//! PORT (leg D, 2026-11-27): this module is tessera 0.1.1
//! (repolex-ai/tessera @ 1142eb66066624ebfa405b7679b3731e72b8fc4f)
//! ported in-tree, file for file (verify / keyring / access), with one
//! substitution: ed25519-dalek's two operations are served by
//! `crate::ed25519` (same verify_strict semantics), because dalek's
//! tree carries a second syn major version and ashfall requires a
//! duplicate-free tree. Everything else is the upstream text.
//!
//! tessera checks who is asking a Subtexture daemon, and whether they may read
//! a named graph.
//!
//! A *tessera* was the small clay, bone or lead admission token handed to a
//! guest or citizen in antiquity and checked at the gate of the theatre. In the
//! Subtexture stack, `git-lex` binds keys and signs tokens and service grants;
//! every daemon (`syrinx`, `pand`, `gitlexd`, `raveld`, `horaed`) checks them at
//! its own door through this library.
//!
//! It does three jobs:
//!
//! - [`verify`]: check an `Authorization: Bearer <token>` header against a key
//!   set (`jwks.json`) and, for a service acting on someone's behalf, a grant
//!   list (`grants.json`). Pure, synchronous, no file or network access.
//! - [`keyring`]: read and refresh `~/.lex/identity/{jwks.json, grants.json,
//!   owner.json}` from disk, or stand down cleanly (`Keyring::Off`) when an
//!   engine such as Pan runs standalone without Subtexture.
//! - [`access`]: decide whether a verified (or unidentified) caller may read a
//!   named graph from its `subtexture:owner`, `subtexture:visibility`, and
//!   `subtexture:readableBy` metadata plus the machine's `Owner`, returning
//!   `401` vs `403` and RFC 6750 `WWW-Authenticate` headers on refusal.

pub mod access;
pub mod keyring;
pub mod verify;

pub use access::{check_read, require_scope, Denial, GraphAccess, Visibility};
pub use keyring::{HostKeys, Keyring, KeyringStatus, Owner};
pub use verify::{
    bearer, verify_caller, verify_caller_with_policy, verify_plain, verify_plain_with_policy,
    www_authenticate_insufficient_scope, www_authenticate_invalid_token, Caller, GrantRecord,
    Grants, Jwks, Key, Policy, AUDIENCE, GRANT_TYP, MAX_GRANT_SECS, MAX_LIFETIME_SECS,
    MIN_GRANT_SECS, PLATFORM_ROOT, SKEW_SECS, SOUL_IRI,
};

/// Why a token, key list or configuration was refused. Written in plain words
/// suitable for returning to the caller or operator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<String> for Error {
    fn from(s: String) -> Self {
        Error(s)
    }
}

impl From<&str> for Error {
    fn from(s: &str) -> Self {
        Error(s.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
