//! raveld's configuration: `~/.config/ravel/config.yml`, one per machine.
//!
//! ```yaml
//! port: 7881
//! interval_secs: 30
//! souls:
//!   - ~/repos/7R1PL3F0RC3/spaceGOAT
//!   - ~/repos/7R1PL3F0RC3/W4R3Z
//! human: Rob                # optional; the human the souls work with, by name
//! memory:                   # optional; without it raveld never calls a model
//!   writer: claude-code     # the harness whose command line writes the memory
//!   model: claude-haiku-5-5 # optional; the writer's default otherwise
//!   reword: on              # optional: rewrite merges left in an older wording,
//!                           # `on`, `off` (the default), or a list of soul names
//! ```
//!
//! A missing file is not an error: the daemon starts with no souls and says
//! so in `/health` by name. "No config file" and "a config with zero souls"
//! must never be the same silent state.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Ruled 2026-09-22 (goodlux): 7881 for now.
pub const DEFAULT_PORT: u16 = 7881;
/// How often the daemon looks for new or grown transcripts. A pass over an
/// unchanged soul costs about a second (the ingest manifest skips everything
/// it has seen), so this can be short.
pub const DEFAULT_INTERVAL_SECS: u64 = 30;

/// The parsed config file, before defaults and meaning are applied.
#[derive(Debug, Clone, Default)]
struct Raw {
    port: Option<u16>,
    interval_secs: Option<u64>,
    souls: Vec<String>,
    human: Option<String>,
    memory: Option<RawMemory>,
    /// Retired 2026-11-05 with the API path. Read only to say so.
    memory_budget_usd: Option<Yaml>,
}

#[derive(Debug, Clone, Default)]
struct RawMemory {
    writer: Option<String>,
    model: Option<String>,
    reword: Option<Yaml>,
}

/// The slice of YAML this file is written in — nothing more.
///
/// serde_yaml (and the C transliteration under it, unsafe-libyaml)
/// left the tree in leg D (2026-11-27): the config's whole grammar is
/// top-level `key: value` lines, one nested map (`memory:`), string
/// lists (block or `[flow]`), comments, and plain scalars, and
/// auditing a C-derived parser line-by-line was not an honest pass.
/// This reader covers exactly those shapes and refuses the rest
/// (anchors, multi-line strings, flow maps) rather than guessing.
/// Matched against serde_yaml where they overlap: `true`/`false` are
/// booleans, `on`/`off` are strings, unknown keys are ignored, a key
/// with no value and no block under it is null.
#[derive(Debug, Clone, PartialEq)]
enum Yaml {
    Null,
    Bool(bool),
    Str(String),
    Seq(Vec<Yaml>),
    Map(Vec<(String, Yaml)>),
}

impl Yaml {
    fn as_str(&self) -> Option<&str> {
        match self {
            Yaml::Str(s) => Some(s),
            _ => None,
        }
    }
    fn get(&self, key: &str) -> Option<&Yaml> {
        match self {
            Yaml::Map(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
}

fn strip_comment(line: &str) -> &str {
    let (mut sq, mut dq) = (false, false);
    let b = line.as_bytes();
    for i in 0..b.len() {
        match b[i] {
            b'\'' if !dq => sq = !sq,
            b'"' if !sq => dq = !dq,
            b'#' if !sq && !dq && (i == 0 || b[i - 1] == b' ' || b[i - 1] == b'\t') => {
                return line[..i].trim_end();
            }
            _ => {}
        }
    }
    line.trim_end()
}

fn yaml_lines(text: &str) -> Vec<(usize, String)> {
    text.lines()
        .map(|l| {
            let clean = strip_comment(l);
            let indent = clean.len() - clean.trim_start().len();
            (indent, clean.trim().to_string())
        })
        .filter(|(_, c)| !c.is_empty())
        .collect()
}

fn parse_scalar(s: &str) -> Yaml {
    let s = s.trim();
    if s.is_empty() {
        return Yaml::Null;
    }
    if let Some(inner) = s.strip_prefix('[').and_then(|x| x.strip_suffix(']')) {
        let inner = inner.trim();
        if inner.is_empty() {
            return Yaml::Seq(vec![]);
        }
        return Yaml::Seq(inner.split(',').map(parse_scalar).collect());
    }
    if let Some(inner) = s.strip_prefix('"').and_then(|x| x.strip_suffix('"')) {
        return Yaml::Str(inner.replace("\\\"", "\"").replace("\\\\", "\\"));
    }
    if let Some(inner) = s.strip_prefix('\'').and_then(|x| x.strip_suffix('\'')) {
        return Yaml::Str(inner.replace("''", "'"));
    }
    match s {
        "true" => Yaml::Bool(true),
        "false" => Yaml::Bool(false),
        "~" | "null" => Yaml::Null,
        _ => Yaml::Str(s.to_string()),
    }
}

/// Parse one block (map or sequence) at `indent`, from `lines[pos..]`;
/// returns the value and the first unconsumed line index.
fn parse_block(lines: &[(usize, String)], pos: usize, indent: usize) -> (Yaml, usize) {
    let mut pos = pos;
    if lines.get(pos).is_some_and(|(_, c)| c == "-" || c.starts_with("- ")) {
        let mut items = Vec::new();
        while let Some((ind, content)) = lines.get(pos) {
            if *ind != indent || !(content == "-" || content.starts_with("- ")) {
                break;
            }
            let item = content.strip_prefix('-').unwrap_or("").trim();
            items.push(parse_scalar(item));
            pos += 1;
        }
        return (Yaml::Seq(items), pos);
    }
    let mut pairs = Vec::new();
    while let Some((ind, content)) = lines.get(pos) {
        if *ind != indent {
            break;
        }
        let Some(colon) = content
            .char_indices()
            .find(|(_, c)| *c == ':')
            .map(|(i, _)| i)
            .filter(|i| {
                let rest = &content[*i + 1..];
                rest.is_empty() || rest.starts_with(' ')
            })
        else {
            break;
        };
        let key = parse_scalar(&content[..colon]);
        let Yaml::Str(key) = key else { break };
        let rest = content[colon + 1..].trim();
        pos += 1;
        if !rest.is_empty() {
            pairs.push((key, parse_scalar(rest)));
            continue;
        }
        // No inline value: a deeper block, a sequence at this same
        // indent (YAML allows that under a key), or null.
        match lines.get(pos) {
            Some((nind, _)) if *nind > indent => {
                let (v, np) = parse_block(lines, pos, *nind);
                pairs.push((key, v));
                pos = np;
            }
            Some((nind, c)) if *nind == indent && (c == "-" || c.starts_with("- ")) => {
                let (v, np) = parse_block(lines, pos, indent);
                pairs.push((key, v));
                pos = np;
            }
            _ => pairs.push((key, Yaml::Null)),
        }
    }
    (Yaml::Map(pairs), pos)
}

fn parse_config_yaml(text: &str) -> Result<Raw> {
    let lines = yaml_lines(text);
    if lines.is_empty() {
        return Ok(Raw::default());
    }
    let (doc, used) = parse_block(&lines, 0, lines[0].0);
    anyhow::ensure!(
        used == lines.len(),
        "config lines past {} are outside the shapes read here",
        used + 1
    );
    let num = |key: &str| -> Result<Option<u64>> {
        match doc.get(key) {
            None | Some(Yaml::Null) => Ok(None),
            Some(Yaml::Str(v)) => v
                .trim()
                .parse::<u64>()
                .map(Some)
                .with_context(|| format!("{key} is a number, not {v:?}")),
            Some(other) => anyhow::bail!("{key} is a number, not {other:?}"),
        }
    };
    let strings = |v: Option<&Yaml>| -> Vec<String> {
        match v {
            Some(Yaml::Seq(items)) => {
                items.iter().filter_map(|i| i.as_str().map(str::to_string)).collect()
            }
            Some(Yaml::Str(one)) => vec![one.clone()],
            _ => vec![],
        }
    };
    let memory = match doc.get("memory") {
        None | Some(Yaml::Null) => None,
        Some(m @ Yaml::Map(_)) => Some(RawMemory {
            writer: m.get("writer").and_then(Yaml::as_str).map(str::to_string),
            model: m.get("model").and_then(Yaml::as_str).map(str::to_string),
            reword: m.get("reword").cloned(),
        }),
        Some(other) => anyhow::bail!("memory: is a map of writer/model/reword, not {other:?}"),
    };
    Ok(Raw {
        port: num("port")?.map(|n| n as u16),
        interval_secs: num("interval_secs")?,
        souls: strings(doc.get("souls")),
        human: doc.get("human").and_then(Yaml::as_str).map(str::to_string),
        memory,
        memory_budget_usd: doc.get("memory_budget_usd").cloned(),
    })
}

/// Which souls have their merges rewritten into the current wording.
#[derive(Debug, Clone, PartialEq)]
pub enum Reword {
    Off,
    All,
    /// By soul name (the repo folder's), so a big soul can wait its turn.
    Souls(Vec<String>),
}

impl Reword {
    pub fn covers(&self, name: &str) -> bool {
        match self {
            Reword::Off => false,
            Reword::All => true,
            Reword::Souls(v) => v.iter().any(|n| n == name),
        }
    }
}

/// Who writes the souls' memories, and with which model.
#[derive(Debug, Clone)]
pub struct MemoryWriter {
    pub harness: crate::writer::Harness,
    pub model: String,
    pub reword: Reword,
}

#[derive(Debug, Clone)]
pub struct DaemonConfig {
    /// Where the file is (or would be).
    pub path: PathBuf,
    /// Whether it was there.
    pub present: bool,
    pub port: u16,
    pub interval_secs: u64,
    /// Soul repo paths, `~` expanded, in file order.
    pub souls: Vec<PathBuf>,
    /// The human these souls work with, by name: how a memory reader labels
    /// the human's turns. `None`: "human".
    pub human: Option<String>,
    /// Who keeps the memories up to date. `None`: nobody, and raveld never
    /// calls a model — a machine's subscription is spent only when a human
    /// wrote `memory:` here (goodlux, 2026-11-05).
    pub memory: Option<MemoryWriter>,
    /// Lines of the file that parse but do nothing, said on every /health.
    pub warnings: Vec<String>,
}

pub fn config_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".config").join("ravel"))
        .unwrap_or_else(|| PathBuf::from(".config/ravel"))
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.yml")
}

/// Where a detached raveld writes what it would have said to a terminal.
pub fn log_path() -> PathBuf {
    config_dir().join("raveld.log")
}

/// The log's cap: past [`LOG_CAP`] bytes raveld keeps only its newest
/// [`LOG_KEEP`], so a log nobody remembers exists cannot fill the disk
/// (goodlux, 2026-11-10).
pub const LOG_CAP: u64 = 100 * 1024 * 1024;
pub const LOG_KEEP: u64 = 50 * 1024 * 1024;

/// Trim the log at `path` to its newest `keep` bytes, starting on a whole
/// line, once it is over `cap`. In place: a detached raveld writes it in
/// append mode, so its next line lands at the new end. True when it trimmed.
pub fn cap_log(path: &std::path::Path, cap: u64, keep: u64) -> std::io::Result<bool> {
    use std::io::{Read, Seek, SeekFrom, Write};
    let len = match std::fs::metadata(path) {
        Ok(m) => m.len(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e),
    };
    if len <= cap {
        return Ok(false);
    }
    let mut f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)?;
    f.seek(SeekFrom::Start(len - keep.min(len)))?;
    let mut tail = Vec::new();
    f.read_to_end(&mut tail)?;
    let start = tail.iter().position(|&b| b == b'\n').map_or(0, |i| i + 1);
    f.set_len(0)?;
    f.seek(SeekFrom::Start(0))?;
    f.write_all(&tail[start..])?;
    Ok(true)
}

pub fn expand_tilde(p: &str) -> PathBuf {
    if let Some(home) = std::env::var_os("HOME") {
        if p == "~" {
            return PathBuf::from(home);
        }
        if let Some(rest) = p.strip_prefix("~/") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(p)
}

impl DaemonConfig {
    pub fn load() -> Result<Self> {
        Self::load_from(&config_path())
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        let (raw, present) = match std::fs::read_to_string(path) {
            Ok(s) if s.trim().is_empty() => (Raw::default(), true),
            Ok(s) => (
                parse_config_yaml(&s)
                    .with_context(|| format!("parse {}", path.display()))?,
                true,
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Raw::default(), false),
            Err(e) => return Err(e).with_context(|| format!("read {}", path.display())),
        };
        let memory = match raw.memory.as_ref().and_then(|m| m.writer.as_deref()) {
            None | Some("none") => None,
            Some(w) => {
                let harness = crate::writer::Harness::parse(w)
                    .with_context(|| format!("memory.writer in {}", path.display()))?;
                let model = raw
                    .memory
                    .as_ref()
                    .and_then(|m| m.model.clone())
                    .filter(|m| !m.trim().is_empty())
                    .unwrap_or_else(|| harness.default_model().to_string());
                let reword = match raw.memory.as_ref().and_then(|m| m.reword.clone()) {
                    None => Reword::Off,
                    Some(Yaml::Bool(b)) => {
                        if b {
                            Reword::All
                        } else {
                            Reword::Off
                        }
                    }
                    Some(Yaml::Str(s)) if s == "on" || s == "all" => Reword::All,
                    Some(Yaml::Str(s)) if s == "off" => Reword::Off,
                    Some(Yaml::Seq(v)) => Reword::Souls(
                        v.iter()
                            .filter_map(|x| x.as_str().map(str::to_string))
                            .collect(),
                    ),
                    Some(other) => anyhow::bail!(
                        "memory.reword in {} is on, off or a list of soul names, not {other:?}",
                        path.display()
                    ),
                };
                Some(MemoryWriter {
                    harness,
                    model,
                    reword,
                })
            }
        };
        let mut warnings = Vec::new();
        if raw.memory_budget_usd.is_some() {
            warnings.push("memory_budget_usd is no longer read: ravel no longer calls the API. Remove the line; `memory:` says who writes the memories".to_string());
        }
        Ok(Self {
            path: path.to_path_buf(),
            present,
            port: raw.port.unwrap_or(DEFAULT_PORT),
            interval_secs: raw.interval_secs.unwrap_or(DEFAULT_INTERVAL_SECS),
            souls: raw.souls.iter().map(|s| expand_tilde(s)).collect(),
            human: raw.human.filter(|h| !h.trim().is_empty()),
            memory,
            warnings,
        })
    }

    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_cap_keeps_the_newest_whole_lines_and_appends_after() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("ravel-logcap-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("raveld.log");
        std::fs::write(&p, "line one\nline two\nline three\n").unwrap();
        // Under the cap: untouched.
        assert!(!cap_log(&p, 100, 10).unwrap());
        // Over it: the newest 12 bytes, from the first whole line in them.
        let mut held = std::fs::OpenOptions::new().append(true).open(&p).unwrap();
        assert!(cap_log(&p, 20, 12).unwrap());
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "line three\n");
        // The writer raveld already holds lands at the new end.
        writeln!(held, "line four").unwrap();
        assert_eq!(
            std::fs::read_to_string(&p).unwrap(),
            "line three\nline four\n"
        );
        assert!(!cap_log(&dir.join("absent.log"), 1, 1).unwrap());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_file_is_defaults_and_says_so() {
        let cfg = DaemonConfig::load_from(Path::new("/nonexistent/ravel/config.yml")).unwrap();
        assert!(!cfg.present);
        assert_eq!(cfg.port, DEFAULT_PORT);
        assert_eq!(cfg.interval_secs, DEFAULT_INTERVAL_SECS);
        assert!(cfg.souls.is_empty());
    }

    #[test]
    fn file_parses_and_expands_tilde() {
        let dir = std::env::temp_dir().join("ravel-config-test");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("config.yml");
        std::fs::write(&p, "port: 9999\nsouls:\n  - ~/repos/x\n  - /abs/y\n").unwrap();
        let cfg = DaemonConfig::load_from(&p).unwrap();
        assert!(cfg.present);
        assert_eq!(cfg.port, 9999);
        assert_eq!(cfg.interval_secs, DEFAULT_INTERVAL_SECS);
        let home = PathBuf::from(std::env::var_os("HOME").unwrap());
        assert_eq!(cfg.souls[0], home.join("repos/x"));
        assert_eq!(cfg.souls[1], PathBuf::from("/abs/y"));
    }

    #[test]
    fn memory_writer_is_off_unless_named_and_an_unknown_one_refuses() {
        let dir = std::env::temp_dir().join("ravel-config-test-memory");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("config.yml");
        std::fs::write(&p, "souls: []\nmemory_budget_usd: 0\n").unwrap();
        let cfg = DaemonConfig::load_from(&p).unwrap();
        assert!(cfg.memory.is_none());
        assert_eq!(cfg.warnings.len(), 1, "a retired key is said, not ignored");
        std::fs::write(&p, "memory:\n  writer: claude-code\n").unwrap();
        let m = DaemonConfig::load_from(&p).unwrap().memory.unwrap();
        assert_eq!(m.harness, crate::writer::Harness::ClaudeCode);
        assert_eq!(m.model, "claude-haiku-5-5");
        std::fs::write(&p, "memory:\n  writer: codex\n  model: gpt-x\n").unwrap();
        assert_eq!(
            DaemonConfig::load_from(&p).unwrap().memory.unwrap().model,
            "gpt-x"
        );
        std::fs::write(
            &p,
            "memory:\n  writer: claude-code\n  reword: [m4xx, h4nk]\n",
        )
        .unwrap();
        let m = DaemonConfig::load_from(&p).unwrap().memory.unwrap();
        assert!(m.reword.covers("h4nk") && !m.reword.covers("lUX"));
        std::fs::write(&p, "memory:\n  writer: claude-code\n  reword: on\n").unwrap();
        let m = DaemonConfig::load_from(&p).unwrap().memory.unwrap();
        assert_eq!(m.reword, Reword::All);
        std::fs::write(&p, "memory:\n  writer: vim\n").unwrap();
        assert!(DaemonConfig::load_from(&p).is_err());
    }

    #[test]
    fn empty_file_is_present_with_defaults() {
        let dir = std::env::temp_dir().join("ravel-config-test-empty");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("config.yml");
        std::fs::write(&p, "\n").unwrap();
        let cfg = DaemonConfig::load_from(&p).unwrap();
        assert!(cfg.present);
        assert!(cfg.souls.is_empty());
    }
}
