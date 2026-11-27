//! A tiny HTTP/1.1 server layer on bare tokio — the one HTTP stack
//! for this repo's two servers (the phext-serve binary below and
//! raveld, which path-depends on this library).
//!
//! Extracted from the phext-serve binary's hand-rolled layer (itself
//! following phext-edit's pattern): one accept loop, one task per
//! connection, request line + headers + a Content-Length body in, a
//! status + Content-Type + body out, `Connection: close` always. No
//! web framework: the route surfaces here are a dozen routes each,
//! and a framework's tree is what ashfall budgets against.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// One request, parsed.
#[derive(Debug, Clone)]
pub struct Request {
    /// Method as sent (`GET`, `POST`, ...).
    pub method: String,
    /// The path, without the query string.
    pub path: String,
    /// The raw query string (after `?`), possibly empty.
    pub query: String,
    /// Headers, names lowercased.
    pub headers: HashMap<String, String>,
    /// The body (Content-Length bytes; chunked bodies are not read —
    /// no client of these servers sends one).
    pub body: Vec<u8>,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(&name.to_lowercase()).map(|v| v.as_str())
    }

    /// The query string as decoded key/value pairs.
    pub fn query_params(&self) -> HashMap<String, String> {
        parse_query_str(&self.query)
    }
}

/// One response.
#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub content_type: String,
    pub extra: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn text(status: u16, body: impl Into<String>) -> Response {
        Response {
            status,
            content_type: "text/plain; charset=utf-8".into(),
            extra: Vec::new(),
            body: body.into().into_bytes(),
        }
    }
    pub fn json(status: u16, v: serde_json::Value) -> Response {
        Response {
            status,
            content_type: "application/json".into(),
            extra: Vec::new(),
            body: v.to_string().into_bytes(),
        }
    }
    pub fn with_header(mut self, name: &str, value: &str) -> Response {
        self.extra.push((name.to_string(), value.to_string()));
        self
    }
}

/// Percent-decode (the query-string form: `+` is a space).
pub fn pct_decode(s: &str) -> String {
    pct_decode_inner(s, true)
}

/// Percent-decode a path segment (`+` stays a plus, as in a path).
pub fn pct_decode_path(s: &str) -> String {
    pct_decode_inner(s, false)
}

fn pct_decode_inner(s: &str, plus_is_space: bool) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => {
                let h = (b[i + 1] as char).to_digit(16);
                let l = (b[i + 2] as char).to_digit(16);
                if let (Some(h), Some(l)) = (h, l) {
                    out.push((h * 16 + l) as u8);
                    i += 3;
                } else {
                    out.push(b[i]);
                    i += 1;
                }
            }
            b'+' if plus_is_space => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Parse a query string into decoded key/value pairs.
pub fn parse_query_str(qs: &str) -> HashMap<String, String> {
    let mut m = HashMap::new();
    for pair in qs.split('&').filter(|p| !p.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        m.insert(pct_decode(k), pct_decode(v));
    }
    m
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        415 => "Unsupported Media Type",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Error",
    }
}

async fn handle<H, Fut>(mut stream: TcpStream, handler: Arc<H>)
where
    H: Fn(Request) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Response> + Send + 'static,
{
    let _ = async {
        let mut buf: Vec<u8> = Vec::with_capacity(8192);
        let mut chunk = [0u8; 8192];
        let head_end = loop {
            let n = stream.read(&mut chunk).await?;
            if n == 0 {
                return Ok::<(), std::io::Error>(());
            }
            buf.extend_from_slice(&chunk[..n]);
            if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                break pos + 4;
            }
            if buf.len() > 1 << 20 {
                return Ok(());
            }
        };
        let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
        let mut lines = head.lines();
        let request_line = lines.next().unwrap_or("");
        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or("").to_string();
        let target = parts.next().unwrap_or("").to_string();
        let mut headers: HashMap<String, String> = HashMap::new();
        for line in lines {
            if let Some((k, v)) = line.split_once(':') {
                headers.insert(k.trim().to_lowercase(), v.trim().to_string());
            }
        }
        let content_len: usize = headers
            .get("content-length")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let mut body: Vec<u8> = buf[head_end..].to_vec();
        while body.len() < content_len {
            let n = stream.read(&mut chunk).await?;
            if n == 0 {
                break;
            }
            body.extend_from_slice(&chunk[..n]);
        }
        body.truncate(content_len);
        let (path, query) = match target.split_once('?') {
            Some((p, q)) => (p.to_string(), q.to_string()),
            None => (target, String::new()),
        };
        let req = Request {
            method,
            path,
            query,
            headers,
            body,
        };
        let resp = handler(req).await;
        let mut head_out = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n",
            resp.status,
            reason(resp.status),
            resp.content_type,
            resp.body.len()
        );
        for (k, v) in &resp.extra {
            head_out.push_str(&format!("{k}: {v}\r\n"));
        }
        head_out.push_str("\r\n");
        stream.write_all(head_out.as_bytes()).await?;
        stream.write_all(&resp.body).await?;
        stream.flush().await?;
        Ok(())
    }
    .await;
}

/// Accept connections forever, one task each, answering with
/// `handler`. Runs until the future is dropped or `shutdown` (see
/// [`serve_with_shutdown`]) resolves.
pub async fn serve<H, Fut>(listener: TcpListener, handler: H)
where
    H: Fn(Request) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Response> + Send + 'static,
{
    serve_with_shutdown(listener, handler, std::future::pending::<()>()).await
}

/// [`serve`], returning once `shutdown` resolves. In-flight
/// connections are not drained (the phext-serve pattern); callers
/// that answer-then-stop arrange their own grace (raveld's /shutdown
/// handler delays its stop signal by 250 ms so its answer lands).
pub async fn serve_with_shutdown<H, Fut, S>(listener: TcpListener, handler: H, shutdown: S)
where
    H: Fn(Request) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Response> + Send + 'static,
    S: std::future::Future<Output = ()> + Send,
{
    let handler = Arc::new(handler);
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                match accepted {
                    Ok((stream, _)) => {
                        let h = handler.clone();
                        tokio::spawn(async move { handle(stream, h).await });
                    }
                    Err(_) => continue,
                }
            }
            _ = &mut shutdown => break,
        }
    }
}
