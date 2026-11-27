//! A small synchronous HTTP/1.1 client for the one server ravel
//! talks to over plain HTTP: raveld itself, on 127.0.0.1.
//!
//! This was reqwest's job until leg D (2026-11-27): reqwest's tree
//! (127 packages with its rustls/quinn stack) could not pass ashfall,
//! and every call site here is a plain-HTTP request to the local
//! daemon — GET or POST, JSON in, JSON out, a bearer token, done.
//! HTTPS is deliberately not implemented here; the tree's one HTTPS
//! call (the Anthropic API, in `crate::writer`) goes through a
//! Python-stdlib helper instead (see there for why).
//!
//! Behaviour notes against the reqwest client it replaces: no
//! redirects are followed (raveld never redirects), and the body is
//! read to close or per Content-Length / chunked framing, which is
//! what the daemon's `Connection: close` server sends.

use anyhow::{anyhow, Context, Result};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

/// One response.
#[derive(Debug)]
pub struct Response {
    pub status: u16,
    /// The reason phrase from the status line (`Not Found`, ...).
    pub reason: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    /// reqwest's `StatusCode::as_str` shape: `"404 Not Found"`.
    pub fn status_text(&self) -> String {
        format!("{} {}", self.status, self.reason)
    }
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
    pub fn json(&self) -> Option<serde_json::Value> {
        serde_json::from_slice(&self.body).ok()
    }
}

/// One request. `url` must be `http://host[:port]/path...`; headers
/// are sent as given; `body` is sent verbatim with a Content-Length.
pub fn request(
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: Option<&[u8]>,
    timeout: Duration,
) -> Result<Response> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| anyhow!("only http:// URLs are read here, not {url}"))?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let addr = authority
        .to_socket_addrs()
        .with_context(|| format!("resolving {authority}"))?
        .next()
        .ok_or_else(|| anyhow!("{authority} does not resolve"))?;
    let mut stream = TcpStream::connect_timeout(&addr, timeout)
        .with_context(|| format!("connecting to {authority}"))?;
    stream.set_read_timeout(Some(timeout)).ok();
    stream.set_write_timeout(Some(timeout)).ok();
    let mut out = format!(
        "{method} {path} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\n"
    );
    for (k, v) in headers {
        out.push_str(&format!("{k}: {v}\r\n"));
    }
    if let Some(b) = body {
        out.push_str(&format!("Content-Length: {}\r\n", b.len()));
    }
    out.push_str("\r\n");
    stream.write_all(out.as_bytes())?;
    if let Some(b) = body {
        stream.write_all(b)?;
    }
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw)?;
    parse_response(&raw).ok_or_else(|| anyhow!("{authority} answered with no readable HTTP response"))
}

fn parse_response(raw: &[u8]) -> Option<Response> {
    let head_end = raw.windows(4).position(|w| w == b"\r\n\r\n")? + 4;
    let head = String::from_utf8_lossy(&raw[..head_end]).into_owned();
    let mut lines = head.lines();
    let status_line = lines.next()?;
    let mut parts = status_line.splitn(3, ' ');
    let _version = parts.next()?;
    let status: u16 = parts.next()?.parse().ok()?;
    let reason = parts.next().unwrap_or("").to_string();
    let mut headers = Vec::new();
    let mut chunked = false;
    let mut content_length: Option<usize> = None;
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            let (k, v) = (k.trim().to_lowercase(), v.trim().to_string());
            if k == "transfer-encoding" && v.to_lowercase().contains("chunked") {
                chunked = true;
            }
            if k == "content-length" {
                content_length = v.parse().ok();
            }
            headers.push((k, v));
        }
    }
    let rest = &raw[head_end..];
    let body = if chunked {
        decode_chunked(rest)
    } else if let Some(n) = content_length {
        rest[..n.min(rest.len())].to_vec()
    } else {
        rest.to_vec()
    };
    Some(Response {
        status,
        reason,
        headers,
        body,
    })
}

fn decode_chunked(mut rest: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let Some(eol) = rest.windows(2).position(|w| w == b"\r\n") else {
            break;
        };
        let size_str = String::from_utf8_lossy(&rest[..eol]).into_owned();
        let Ok(size) = usize::from_str_radix(size_str.trim().split(';').next().unwrap_or(""), 16)
        else {
            break;
        };
        rest = &rest[eol + 2..];
        if size == 0 {
            break;
        }
        let take = size.min(rest.len());
        out.extend_from_slice(&rest[..take]);
        rest = &rest[take..];
        if rest.starts_with(b"\r\n") {
            rest = &rest[2..];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_content_length_and_chunked() {
        let r = parse_response(
            b"HTTP/1.1 404 Not Found\r\nContent-Length: 5\r\n\r\nhelloEXTRA",
        )
        .unwrap();
        assert_eq!(r.status, 404);
        assert_eq!(r.status_text(), "404 Not Found");
        assert_eq!(r.body, b"hello");
        let r = parse_response(
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n",
        )
        .unwrap();
        assert_eq!(r.body, b"hello world");
    }
}
