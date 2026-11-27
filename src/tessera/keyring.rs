//! Reading keys, grants and machine ownership from `~/.lex/identity/`.
//!
//! [`HostKeys`] caches `jwks.json` in memory and re-reads it automatically when
//! an unknown `kid` arrives (a squaddie bound since startup) or when a token is
//! refused and the file on disk has changed (a squaddie whose `sub` address
//! switched while the daemon was running). `grants.json` is read on every
//! service-token check so `git lex identity revoke` takes effect at once.
//!
//! [`Keyring`] wraps [`HostKeys`] with an [`Keyring::Off`] variant so engines
//! that also run standalone without Subtexture (such as Pan) share one type.

use super::verify::{bearer, verify_caller_with_policy, Caller, Grants, Jwks, Key, Policy};
use super::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::RwLock;

impl Policy {
    /// Read `policy.json` at `path`. A missing file returns
    /// `Ok(Policy::default())`; a file that cannot be read, does not parse, or
    /// sets incoherent bounds returns `Err` (never silently ignored).
    pub fn load_file(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::from_json(&text)
                .map_err(|e| Error(format!("{}: {e}", path.display()))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Policy::default()),
            Err(e) => Err(Error(format!("cannot read {}: {e}", path.display()))),
        }
    }
}

/// The owner of this machine (`~/.lex/identity/owner.json`), recorded by
/// `git lex identity owner` (goodlux, 2026-11-16). The machine owner is a
/// person's identity rather than an agent's and may read every store on the
/// machine.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Owner {
    /// The owner's `sub` address at the time `owner.json` was written.
    pub owner: String,
    /// The 40-hex first commit (`kid`) of the owner's repository. Stable across
    /// username and squad address changes.
    pub genesis: String,
    /// Unix seconds when ownership was recorded.
    pub since: u64,
}

impl Owner {
    /// Parse `owner.json` text.
    pub fn from_json(text: &str) -> Result<Self> {
        serde_json::from_str(text)
            .map_err(|e| Error(format!("owner record does not parse as JSON: {e}")))
    }

    /// Read `owner.json` at `path`. A missing file returns `Ok(None)`; a file
    /// that cannot be read or does not parse returns `Err` (never silently
    /// treated as "no owner").
    pub fn load_file(path: &Path) -> Result<Option<Self>> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::from_json(&text)
                .map(Some)
                .map_err(|e| Error(format!("{}: {e}", path.display()))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(Error(format!("cannot read {}: {e}", path.display()))),
        }
    }

    /// Whether `caller` is this machine's owner, matched by stable `genesis`
    /// (`kid`) or by `owner` (`sub`).
    pub fn matches(&self, caller: &Caller) -> bool {
        caller.genesis == self.genesis || caller.subject == self.owner
    }
}

/// Health snapshot of a [`HostKeys`] key list, suitable for `/health` routes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyringStatus {
    pub keys_file: PathBuf,
    pub squaddies_with_a_key: usize,
    pub error: Option<String>,
}

/// Live reader for `~/.lex/identity/jwks.json` (and sibling `grants.json` /
/// `owner.json`).
#[derive(Debug)]
pub struct HostKeys {
    jwks_path: PathBuf,
    cached: RwLock<Jwks>,
}

impl HostKeys {
    /// Create a key reader pointing at `jwks_path` (typically
    /// `~/.lex/identity/jwks.json`) and load whatever keys exist now. If the
    /// file is not there yet, the cache starts empty and will reload on the
    /// first presented token.
    pub fn new(jwks_path: PathBuf) -> Self {
        let hk = HostKeys {
            jwks_path,
            cached: RwLock::new(Jwks::default()),
        };
        let _ = hk.reload();
        hk
    }

    /// Path to `jwks.json`.
    pub fn jwks_path(&self) -> &Path {
        &self.jwks_path
    }

    /// Sibling `grants.json` path.
    pub fn grants_path(&self) -> PathBuf {
        self.jwks_path.with_file_name("grants.json")
    }

    /// Sibling `owner.json` path.
    pub fn owner_path(&self) -> PathBuf {
        self.jwks_path.with_file_name("owner.json")
    }

    /// Sibling `policy.json` path (`~/.lex/identity/policy.json`).
    pub fn policy_path(&self) -> PathBuf {
        self.jwks_path.with_file_name("policy.json")
    }

    /// Re-read `jwks.json` from disk and replace the in-memory cache.
    pub fn reload(&self) -> Result<usize> {
        let text = std::fs::read_to_string(&self.jwks_path).map_err(|e| {
            Error(format!(
                "cannot read the list of squaddies' keys at {}: {e}",
                self.jwks_path.display()
            ))
        })?;
        let jwks = Jwks::from_json(&text).map_err(|e| {
            Error(format!(
                "{} is not a usable key list: {e}",
                self.jwks_path.display()
            ))
        })?;
        let count = jwks.len();
        *self.cached.write().unwrap() = jwks;
        Ok(count)
    }

    /// Refresh and report the key list (and optional `policy.json`) status for
    /// `/health`.
    pub fn status(&self) -> KeyringStatus {
        match (self.reload(), self.policy()) {
            (Ok(n), Ok(_)) => KeyringStatus {
                keys_file: self.jwks_path.clone(),
                squaddies_with_a_key: n,
                error: None,
            },
            (Ok(n), Err(e)) => KeyringStatus {
                keys_file: self.jwks_path.clone(),
                squaddies_with_a_key: n,
                error: Some(e.0),
            },
            (Err(e), _) => KeyringStatus {
                keys_file: self.jwks_path.clone(),
                squaddies_with_a_key: 0,
                error: Some(e.0),
            },
        }
    }

    /// Load the sibling `policy.json`, or [`Policy::default()`] when absent.
    pub fn policy(&self) -> Result<Policy> {
        Policy::load_file(&self.policy_path())
    }

    /// Load the sibling `owner.json`, if recorded on this machine.
    pub fn owner(&self) -> Result<Option<Owner>> {
        Owner::load_file(&self.owner_path())
    }

    /// Load the sibling `grants.json` (used for service tokens).
    pub fn grants(&self) -> Result<Grants> {
        let path = self.grants_path();
        let text = std::fs::read_to_string(&path).map_err(|e| {
            Error(format!(
                "cannot read the list of grants at {}: {e}",
                path.display()
            ))
        })?;
        Grants::from_json(&text).map_err(|e| Error(format!("{}: {e}", path.display())))
    }

    fn key(&self, kid: &str) -> Result<Key> {
        if let Some(k) = self.cached.read().unwrap().key(kid) {
            return Ok(k.clone());
        }
        self.reload()?;
        self.cached
            .read()
            .unwrap()
            .key(kid)
            .cloned()
            .ok_or_else(|| Error(format!("no squaddie on the key list has the id {kid}")))
    }

    /// Identify the caller from an HTTP `Authorization` header.
    ///
    /// - `Ok(None)`: no `Authorization` header was sent.
    /// - `Ok(Some(caller))`: a valid `Bearer` token was presented and verified.
    /// - `Err(reason)`: a header was sent and failed verification (`401`).
    ///
    /// If verification fails on cached keys (for example right after an
    /// identity's `sub` switched on `jwks.json`), `jwks.json` is re-read once
    /// and the token checked again before the refusal stands.
    pub fn identify(&self, authorization: Option<&str>, now: u64) -> Result<Option<Caller>> {
        let Some(token) = bearer(authorization)? else {
            return Ok(None);
        };
        self.verify_token(token, now).map(Some)
    }

    /// Verify a raw JWT string against this host's `jwks.json`, `grants.json`,
    /// and optional `policy.json`.
    pub fn verify_token(&self, token: &str, now: u64) -> Result<Caller> {
        let policy = self.policy()?;
        match verify_caller_with_policy(token, |kid| self.key(kid), || self.grants(), &policy, now)
        {
            Ok(caller) => Ok(caller),
            Err(first) => {
                let before = self.cached.read().unwrap().clone();
                if self.reload().is_err() || *self.cached.read().unwrap() == before {
                    return Err(first);
                }
                verify_caller_with_policy(
                    token,
                    |kid| self.key(kid),
                    || self.grants(),
                    &policy,
                    now,
                )
            }
        }
    }
}

/// Configurable identity keyring for daemons that may run either inside
/// Subtexture or standalone.
#[derive(Debug)]
pub enum Keyring {
    /// Identity is disabled (standalone install with no `identity:` block):
    /// `identify` inspects nothing and every caller is `None`.
    Off,
    /// Subtexture Ed25519 tokens verified against `jwks.json` and `grants.json`.
    Subtexture(HostKeys),
}

impl Keyring {
    /// Build `Keyring::Subtexture` at `~/.lex/identity/jwks.json` under `home`.
    pub fn subtexture_in_home(home: &Path) -> Self {
        Keyring::Subtexture(HostKeys::new(home.join(".lex/identity/jwks.json")))
    }

    /// Build `Keyring::Subtexture` from an explicit `jwks.json` path.
    pub fn subtexture_at(jwks_path: PathBuf) -> Self {
        Keyring::Subtexture(HostKeys::new(jwks_path))
    }

    /// Provider name (`"off"` or `"subtexture"`).
    pub fn provider_name(&self) -> &'static str {
        match self {
            Keyring::Off => "off",
            Keyring::Subtexture(_) => "subtexture",
        }
    }

    /// Identify the caller of a request. Returns `Ok(None)` when `Off` or when
    /// no `Authorization` header was sent.
    pub fn identify(&self, authorization: Option<&str>, now: u64) -> Result<Option<Caller>> {
        match self {
            Keyring::Off => Ok(None),
            Keyring::Subtexture(hk) => hk.identify(authorization, now),
        }
    }

    /// Machine owner from `owner.json`, or `Ok(None)` when `Off` or unset.
    pub fn owner(&self) -> Result<Option<Owner>> {
        match self {
            Keyring::Off => Ok(None),
            Keyring::Subtexture(hk) => hk.owner(),
        }
    }
}
