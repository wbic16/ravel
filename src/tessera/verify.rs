//! Pure verification of Subtexture Ed25519 tokens and service grants.
//!
//! Nothing in this module reads a file or opens a socket. The host passes in a
//! key lookup (`kid -> Key`) and, when a service token arrives, a closure that
//! returns the machine's live grant list (`Grants`).
//!
//! ## Plain identity tokens (`git-lex`, 2026-11-15)
//!
//! Six checks, in order; nothing inside the token chooses what is trusted:
//!
//! 1. `alg` is `"EdDSA"`, fixed in code and never chosen by the header; `typ`
//!    is not `"grant+jwt"`.
//! 2. The header's `kid` selects which listed Ed25519 key to try, and nothing
//!    more.
//! 3. The signature verifies strictly (`VerifyingKey::verify_strict`) against
//!    that listed key.
//! 4. `sub` equals the exact address the key list records for that key.
//! 5. `iat` is at most 60 seconds in the future, `exp` is in the future, and
//!    `exp - iat` is at most 24 hours.
//! 6. `aud` is `"subtexture"`.
//!
//! ## Service tokens carrying a grant (`git-lex`, 2026-11-17)
//!
//! A UI server or background worker never holds a person's or squaddie's key.
//! Instead it holds its own Ed25519 key, and the identity it acts for signs a
//! `grant+jwt` naming that key (`cnf.jwk`, RFC 7800), a service name, a space-
//! separated `scope` and an expiry. When an incoming token carries a `grant` or
//! `act` claim:
//!
//! 1. The inner grant is checked first against the grantor's listed key: EdDSA,
//!    `typ == "grant+jwt"`, `kid` on the key list, strict signature check,
//!    `sub` and `iss` both equal to the grantor's listed address, lifetime
//!    between 2 and 366 days, `aud == "subtexture"`, and an Ed25519 `cnf.jwk`.
//! 2. The grant's `jti` is present on the machine's `Grants` list with `ended`
//!    absent, and the record agrees on `grantor` (`kid`), `service` and `x`.
//! 3. The outer token's signature verifies strictly with `cnf.jwk.x`.
//! 4. `outer.sub == grant.sub`, `outer.act.sub == service`, and
//!    `outer.iss == "service:{service}"`.
//! 5. The outer token's `iat`/`exp` pass the 24-hour window and `exp <= grant.exp`.
//! 6. `outer.aud == "subtexture"`.

use super::{Error, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
/// Adapter over the in-tree Ed25519 verification (`crate::ed25519`),
/// standing where ed25519-dalek stood in tessera: same two operations,
/// same strictness (see that module's header for the rules and why).
struct Signature([u8; 64]);

impl Signature {
    fn from_slice(bytes: &[u8]) -> std::result::Result<Self, ()> {
        let arr: [u8; 64] = bytes.try_into().map_err(|_| ())?;
        Ok(Signature(arr))
    }
}

struct VerifyingKey([u8; 32]);

impl VerifyingKey {
    fn from_bytes(bytes: &[u8; 32]) -> std::result::Result<Self, String> {
        if crate::ed25519::key_is_usable(bytes) {
            Ok(VerifyingKey(*bytes))
        } else {
            Err("the key does not decompress".into())
        }
    }

    fn verify_strict(&self, message: &[u8], signature: &Signature) -> std::result::Result<(), ()> {
        crate::ed25519::verify_strict(&self.0, message, &signature.0)
    }
}
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// The `aud` claim required on every Subtexture token and grant.
pub const AUDIENCE: &str = "subtexture";
/// Platform IRI root for Repolex stores and squads.
pub const PLATFORM_ROOT: &str = "https://repolex.ai/";
/// Unscoped Soul IRI prefix, used for keys listed before store-scoped `sub`
/// addresses were recorded on `jwks.json`.
pub const SOUL_IRI: &str = "https://repolex.ai/soul/Soul/";
/// Default longest token lifetime accepted by a verifier (7 days; overridable
/// at runtime via `~/.lex/identity/policy.json`, goodlux 2026-11-21).
pub const MAX_LIFETIME_SECS: u64 = 7 * 24 * 3600;
/// Clock skew tolerated between signer and verifier (60 seconds).
pub const SKEW_SECS: u64 = 60;
/// Default shortest grant lifetime accepted (8 days — strictly longer than any
/// default plain token).
pub const MIN_GRANT_SECS: u64 = 8 * 24 * 3600;
/// Default longest grant lifetime accepted (366 days).
pub const MAX_GRANT_SECS: u64 = 366 * 24 * 3600;
/// Upper sanity bound on `max_grant_secs` in `policy.json` (10 years).
const HARD_MAX_GRANT_SECS: u64 = 10 * 366 * 24 * 3600;
/// Header `typ` distinguishing a grant from a plain token.
pub const GRANT_TYP: &str = "grant+jwt";

const fn default_max_token_secs() -> u64 {
    MAX_LIFETIME_SECS
}
const fn default_min_grant_secs() -> u64 {
    MIN_GRANT_SECS
}
const fn default_max_grant_secs() -> u64 {
    MAX_GRANT_SECS
}

/// Runtime-configurable token and grant lifetime limits
/// (`~/.lex/identity/policy.json`).
///
/// When `policy.json` is absent, [`Policy::default()`] applies (`7` days max
/// token lifetime, `8` days min grant lifetime, `366` days max grant lifetime).
/// `token_lifetime_secs` is optional so `git-lex` can read its issuing lifetime
/// from the same file without daemon rebuilds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    /// Optional issuing lifetime read by `git-lex` when minting tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_lifetime_secs: Option<u64>,
    /// Maximum lifetime (`exp - iat`) accepted for plain and acting tokens.
    #[serde(default = "default_max_token_secs")]
    pub max_token_secs: u64,
    /// Minimum lifetime (`exp - iat`) required on a `grant+jwt` (`> max_token_secs`).
    #[serde(default = "default_min_grant_secs")]
    pub min_grant_secs: u64,
    /// Maximum lifetime (`exp - iat`) accepted on a `grant+jwt`.
    #[serde(default = "default_max_grant_secs")]
    pub max_grant_secs: u64,
}

impl Default for Policy {
    fn default() -> Self {
        Policy {
            token_lifetime_secs: None,
            max_token_secs: MAX_LIFETIME_SECS,
            min_grant_secs: MIN_GRANT_SECS,
            max_grant_secs: MAX_GRANT_SECS,
        }
    }
}

impl Policy {
    /// Check that lifetime bounds are coherent (`0 < max_token_secs < min_grant_secs <= max_grant_secs`).
    pub fn validate(self) -> Result<Self> {
        if self.max_token_secs == 0 {
            return Err(Error("policy max_token_secs must be greater than 0".into()));
        }
        if let Some(issue) = self.token_lifetime_secs {
            if issue == 0 || issue > self.max_token_secs {
                return Err(Error(format!(
                    "policy token_lifetime_secs ({issue}) must be between 1 and max_token_secs ({})",
                    self.max_token_secs
                )));
            }
        }
        if self.min_grant_secs <= self.max_token_secs {
            return Err(Error(format!(
                "policy min_grant_secs ({}) must be strictly greater than max_token_secs ({})",
                self.min_grant_secs, self.max_token_secs
            )));
        }
        if self.max_grant_secs < self.min_grant_secs || self.max_grant_secs > HARD_MAX_GRANT_SECS {
            return Err(Error(format!(
                "policy max_grant_secs ({}) must be between min_grant_secs ({}) and {HARD_MAX_GRANT_SECS}",
                self.max_grant_secs, self.min_grant_secs
            )));
        }
        Ok(self)
    }

    /// Parse and validate `policy.json` text.
    pub fn from_json(text: &str) -> Result<Self> {
        let parsed: Policy = serde_json::from_str(text)
            .map_err(|e| Error(format!("policy does not parse as JSON: {e}")))?;
        parsed.validate()
    }
}

/// One Ed25519 public key from `jwks.json`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Key {
    /// 40-character hex first-commit hash of the identity's repository.
    pub kid: String,
    /// Raw 32-byte Ed25519 verifying key.
    pub x: [u8; 32],
    /// Exact `sub` address required on tokens and grants signed by this key.
    /// `None` only on key lists written before 2026-11-19.
    pub sub: Option<String>,
    /// One-word username recorded on the key list, when present.
    pub username: Option<String>,
}

impl Key {
    /// The only `sub` (and, in a grant, `iss`) accepted from this key.
    pub fn subject(&self) -> String {
        self.sub
            .clone()
            .unwrap_or_else(|| format!("{SOUL_IRI}{}", self.kid))
    }
}

/// Parsed JSON Web Key Set (`~/.lex/identity/jwks.json`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Jwks {
    keys: HashMap<String, Key>,
}

impl Jwks {
    /// Parse a JWKS JSON document. Non-Ed25519 keys are ignored; a malformed
    /// Ed25519 key or a non-string `sub`/`username` member fails the whole parse
    /// loudly rather than silently dropping or downgrading the entry.
    pub fn from_json(text: &str) -> Result<Self> {
        let v: Value =
            serde_json::from_str(text).map_err(|e| Error(format!("not valid JSON: {e}")))?;
        let list = v
            .get("keys")
            .and_then(Value::as_array)
            .ok_or_else(|| Error("no \"keys\" array in JWKS".into()))?;
        let mut keys = HashMap::with_capacity(list.len());
        for entry in list {
            let str_field = |f: &str| entry.get(f).and_then(Value::as_str);
            if str_field("kty") != Some("OKP") || str_field("crv") != Some("Ed25519") {
                continue;
            }
            let (Some(kid), Some(x_b64)) = (str_field("kid"), str_field("x")) else {
                continue;
            };
            let decoded = URL_SAFE_NO_PAD
                .decode(x_b64)
                .map_err(|e| Error(format!("key {kid}: invalid base64url in \"x\": {e}")))?;
            let x: [u8; 32] = decoded
                .try_into()
                .map_err(|_| Error(format!("key {kid} is not 32 bytes")))?;
            let sub = optional_non_empty_string(entry, "sub", kid)?;
            let username = optional_non_empty_string(entry, "username", kid)?;
            keys.insert(
                kid.to_string(),
                Key {
                    kid: kid.to_string(),
                    x,
                    sub,
                    username,
                },
            );
        }
        Ok(Jwks { keys })
    }

    /// Look up an Ed25519 key by `kid` (the 40-hex genesis commit SHA).
    pub fn key(&self, kid: &str) -> Option<&Key> {
        self.keys.get(kid)
    }

    /// Number of usable Ed25519 keys in the set.
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// Whether the key set holds no Ed25519 keys.
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

fn optional_non_empty_string(entry: &Value, field: &str, kid: &str) -> Result<Option<String>> {
    match entry.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if !s.is_empty() => Ok(Some(s.clone())),
        Some(_) => Err(Error(format!(
            "key {kid}: its \"{field}\" member is not a non-empty string"
        ))),
    }
}

/// One entry in `~/.lex/identity/grants.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrantRecord {
    pub jti: String,
    /// 40-hex first commit (`kid`) of the identity that signed the grant.
    pub grantor: String,
    /// Service name (`act.sub`).
    pub service: String,
    /// Space-separated scope words (`syrinx:read pan:read`).
    #[serde(default)]
    pub scope: String,
    /// Base64url-encoded 32-byte Ed25519 public key of the service.
    pub x: String,
    #[serde(default)]
    pub iat: u64,
    #[serde(default)]
    pub exp: u64,
    /// Unix seconds when revoked; `None` while the grant remains in force.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended: Option<u64>,
}

/// The machine's list of grants (`~/.lex/identity/grants.json`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grants {
    #[serde(default)]
    pub grants: Vec<GrantRecord>,
}

impl Grants {
    /// Parse `grants.json` text.
    pub fn from_json(text: &str) -> Result<Self> {
        serde_json::from_str(text)
            .map_err(|e| Error(format!("grants list does not parse as JSON: {e}")))
    }
}

/// Who a verified token speaks for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Caller {
    /// Verified IRI (`sub`) of the squaddie or person the request is for, as
    /// listed on `jwks.json`.
    pub subject: String,
    /// 40-character hex first-commit SHA (`kid`) of that identity's repo.
    pub genesis: String,
    /// One-word username when listed on `jwks.json` or carried in a store-scoped
    /// `subject`.
    pub username: Option<String>,
    /// Squad name when `subject` has the store-scoped shape
    /// `https://repolex.ai/{squad}/{user}/soul/Soul/{user}`.
    pub squad: Option<String>,
    /// Service name (`act.sub`) when the request arrived under a grant; `None`
    /// when the identity acts for itself.
    pub actor: Option<String>,
    /// Scope words granted to `actor`; `None` when the identity acts for itself.
    pub scope: Option<Vec<String>>,
    /// Unix timestamp (`exp`) when the presented token expires.
    pub expires: u64,
}

impl Caller {
    /// Whether this caller's token permits `required_scope` (for example
    /// `"pan:write"` or `"syrinx:read"`). An identity acting for itself (`scope`
    /// is `None`) is unrestricted by service scopes; a service token must list
    /// the scope word in its grant.
    pub fn allows_scope(&self, required_scope: &str) -> bool {
        self.scope
            .as_ref()
            .map_or(true, |words| words.iter().any(|w| w == required_scope))
    }

    /// Squad IRI (`https://repolex.ai/{squad}`) when this caller belongs to a
    /// squad, for matching against `subtexture:readableBy`.
    pub fn squad_iri(&self) -> Option<String> {
        self.squad
            .as_deref()
            .map(|sq| format!("{PLATFORM_ROOT}{sq}"))
    }
}

/// Parse `(squad, username)` from a store-scoped Soul IRI
/// `https://repolex.ai/{squad}/{user}/soul/Soul/{user}`.
fn parse_store_scoped_soul(subject: &str) -> (Option<String>, Option<String>) {
    let Some(rest) = subject.strip_prefix(PLATFORM_ROOT) else {
        return (None, None);
    };
    let parts: Vec<&str> = rest.split('/').collect();
    match parts.as_slice() {
        [squad, user, "soul", "Soul", leaf]
            if !squad.is_empty() && !user.is_empty() && user == leaf =>
        {
            (Some((*squad).to_string()), Some((*user).to_string()))
        }
        _ => (None, None),
    }
}

/// Extract `<token>` from an `Authorization` header.
///
/// - `None` (no `Authorization` header) returns `Ok(None)`: the caller made no
///   identity claim.
/// - `"Bearer <token>"` (ASCII case-insensitive `"Bearer"`) returns
///   `Ok(Some(token))`.
/// - Any other `Authorization` header (another scheme such as `Basic`, or an
///   empty `Bearer` value) returns `Err`: a malformed header is surfaced as
///   `401`, never silently treated as anonymous.
pub fn bearer(authorization: Option<&str>) -> Result<Option<&str>> {
    let Some(raw) = authorization else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if let Some((scheme, rest)) = trimmed.split_once(char::is_whitespace) {
        if scheme.eq_ignore_ascii_case("bearer") {
            let token = rest.trim();
            if !token.is_empty() {
                return Ok(Some(token));
            }
        }
    }
    Err(Error(
        "the Authorization header is not \"Bearer <token>\", the only form accepted".into(),
    ))
}

/// Format an RFC 6750 `WWW-Authenticate` header value for `401 Unauthorized`
/// when a presented token fails verification.
pub fn www_authenticate_invalid_token(reason: &str) -> String {
    format!(
        "Bearer realm=\"{AUDIENCE}\", error=\"invalid_token\", error_description=\"{}\"",
        sanitize_header_quoted(reason)
    )
}

/// Format an RFC 6750 `WWW-Authenticate` header value for `403 Forbidden` when
/// a service token lacks the required grant scope.
pub fn www_authenticate_insufficient_scope(required_scope: &str) -> String {
    format!(
        "Bearer realm=\"{AUDIENCE}\", error=\"insufficient_scope\", scope=\"{}\"",
        sanitize_header_quoted(required_scope)
    )
}

fn sanitize_header_quoted(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '"' | '\\' => '\'',
            '\r' | '\n' => ' ',
            other => other,
        })
        .collect()
}

struct Jwt<'a> {
    header: Value,
    claims: Value,
    signed: &'a str,
    signature: Signature,
}

fn split_jwt<'a>(token: &'a str, label: &str) -> Result<Jwt<'a>> {
    let mut parts = token.split('.');
    let (Some(h), Some(c), Some(s), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(Error(format!(
            "not a {label}: expected three parts separated by dots"
        )));
    };
    let decode = |part: &str, piece: &str| {
        URL_SAFE_NO_PAD
            .decode(part)
            .map_err(|e| Error(format!("the {label}'s {piece} is not base64url: {e}")))
    };
    let header: Value = serde_json::from_slice(&decode(h, "header")?)
        .map_err(|e| Error(format!("the {label}'s header is not JSON: {e}")))?;
    // Fixed in code: the token's header never chooses the verification algorithm.
    if header.get("alg").and_then(Value::as_str) != Some("EdDSA") {
        return Err(Error(format!(
            "the {label} is not signed with EdDSA, the only signature type accepted"
        )));
    }
    let claims: Value = serde_json::from_slice(&decode(c, "claims")?)
        .map_err(|e| Error(format!("the {label}'s claims are not JSON: {e}")))?;
    let signature = Signature::from_slice(&decode(s, "signature")?)
        .map_err(|_| Error(format!("the {label}'s signature is the wrong length")))?;
    Ok(Jwt {
        header,
        claims,
        signed: &token[..h.len() + 1 + c.len()],
        signature,
    })
}

fn check_times(claims: &Value, now: u64, min: u64, max: u64, label: &str) -> Result<(u64, u64)> {
    let (Some(iat), Some(exp)) = (
        claims.get("iat").and_then(Value::as_u64),
        claims.get("exp").and_then(Value::as_u64),
    ) else {
        return Err(Error(format!(
            "the {label} must carry both iat and exp, as whole seconds"
        )));
    };
    if iat > now + SKEW_SECS {
        return Err(Error(format!("the {label} is dated in the future")));
    }
    if exp <= now {
        return Err(Error(format!("the {label} has expired")));
    }
    if exp < iat || exp - iat > max {
        return Err(Error(format!(
            "the {label}'s lifetime is longer than the {} hours allowed",
            max / 3600
        )));
    }
    if exp - iat < min {
        return Err(Error(format!(
            "the {label}'s lifetime is shorter than the {} hours required",
            min / 3600
        )));
    }
    Ok((iat, exp))
}

fn check_audience(claims: &Value, label: &str) -> Result<()> {
    match claims.get("aud").and_then(Value::as_str) {
        Some(AUDIENCE) => Ok(()),
        Some(other) => Err(Error(format!(
            "the {label} is for {other:?}, not for {AUDIENCE:?}"
        ))),
        None => Err(Error(format!("the {label} is not for {AUDIENCE:?}"))),
    }
}

fn build_caller(
    listed: &Key,
    subject: String,
    actor: Option<String>,
    scope: Option<Vec<String>>,
    expires: u64,
) -> Caller {
    let (squad, parsed_user) = parse_store_scoped_soul(&subject);
    let username = listed.username.clone().or(parsed_user);
    Caller {
        subject,
        genesis: listed.kid.clone(),
        username,
        squad,
        actor,
        scope,
        expires,
    }
}

/// Check any Subtexture token using [`Policy::default()`] lifetime limits.
pub fn verify_caller(
    token: &str,
    key_for: impl Fn(&str) -> Result<Key>,
    grants: impl Fn() -> Result<Grants>,
    now: u64,
) -> Result<Caller> {
    verify_caller_with_policy(token, key_for, grants, &Policy::default(), now)
}

/// Check any Subtexture token against explicit [`Policy`] lifetime limits: an
/// identity's own token, or a service token carrying a grant.
///
/// Which path runs is chosen solely from whether the unverified claims carry
/// `grant` or `act`; nothing else in the unverified claims is trusted before
/// signatures are checked. `grants` is only called on the service path.
pub fn verify_caller_with_policy(
    token: &str,
    key_for: impl Fn(&str) -> Result<Key>,
    grants: impl Fn() -> Result<Grants>,
    policy: &Policy,
    now: u64,
) -> Result<Caller> {
    let policy = (*policy).validate()?;
    let outer = split_jwt(token, "token")?;
    if outer.claims.get("grant").is_none() && outer.claims.get("act").is_none() {
        return verify_plain_parts(&outer, key_for, &policy, now);
    }
    verify_service(&outer, key_for, grants, &policy, now)
}

/// Check an identity's own plain token using [`Policy::default()`] lifetime limits.
pub fn verify_plain(token: &str, key_for: impl Fn(&str) -> Result<Key>, now: u64) -> Result<Caller> {
    verify_plain_with_policy(token, key_for, &Policy::default(), now)
}

/// Check an identity's own plain token (refusing service tokens and grants)
/// against explicit [`Policy`] lifetime limits.
pub fn verify_plain_with_policy(
    token: &str,
    key_for: impl Fn(&str) -> Result<Key>,
    policy: &Policy,
    now: u64,
) -> Result<Caller> {
    let policy = (*policy).validate()?;
    let outer = split_jwt(token, "token")?;
    verify_plain_parts(&outer, key_for, &policy, now)
}

fn verify_plain_parts(
    t: &Jwt<'_>,
    key_for: impl Fn(&str) -> Result<Key>,
    policy: &Policy,
    now: u64,
) -> Result<Caller> {
    // 1. EdDSA was fixed in `split_jwt`; a grant cannot pass as a plain token.
    if t.header.get("typ").and_then(Value::as_str) == Some(GRANT_TYP) {
        return Err(Error(
            "this is a grant, not a token; a service presents it inside its own token".into(),
        ));
    }
    // 2. `kid` selects which listed key to try, and nothing more.
    let kid = t
        .header
        .get("kid")
        .and_then(Value::as_str)
        .ok_or_else(|| Error("the token names no key (no kid)".into()))?;
    let listed = key_for(kid)?;
    let vk = VerifyingKey::from_bytes(&listed.x)
        .map_err(|e| Error(format!("the listed key for {kid} is not usable: {e}")))?;
    // 3. Strict Ed25519 signature check.
    vk.verify_strict(t.signed.as_bytes(), &t.signature)
        .map_err(|_| Error("the signature does not match the listed key for this squaddie".into()))?;
    // 4. `sub` must equal the exact address recorded on the key list.
    let expected_sub = listed.subject();
    if t.claims.get("sub").and_then(Value::as_str) != Some(expected_sub.as_str()) {
        return Err(Error(
            "the token says it is a different squaddie than the one whose key signed it".into(),
        ));
    }
    // 5. Dates and configured maximum token lifetime.
    let (_, exp) = check_times(&t.claims, now, 0, policy.max_token_secs, "token")?;
    // 6. Audience.
    check_audience(&t.claims, "token")?;

    Ok(build_caller(&listed, expected_sub, None, None, exp))
}

fn verify_service(
    outer: &Jwt<'_>,
    key_for: impl Fn(&str) -> Result<Key>,
    grants: impl Fn() -> Result<Grants>,
    policy: &Policy,
    now: u64,
) -> Result<Caller> {
    let text = |v: &Value, f: &str| v.get(f).and_then(Value::as_str).map(str::to_string);

    // 1. Check the carried grant with the grantor's listed key.
    let carried = text(&outer.claims, "grant")
        .ok_or_else(|| Error("the service's token carries no grant".into()))?;
    let g = split_jwt(&carried, "grant")?;
    if g.header.get("typ").and_then(Value::as_str) != Some(GRANT_TYP) {
        return Err(Error(
            "what the token carries as its grant is not a grant".into(),
        ));
    }
    let kid = g
        .header
        .get("kid")
        .and_then(Value::as_str)
        .ok_or_else(|| Error("the grant names no key (no kid)".into()))?;
    let listed = key_for(kid)?;
    VerifyingKey::from_bytes(&listed.x)
        .map_err(|e| Error(format!("the listed key for {kid} is not usable: {e}")))?
        .verify_strict(g.signed.as_bytes(), &g.signature)
        .map_err(|_| {
            Error(
                "the grant's signature does not match the listed key of the one it says granted it"
                    .into(),
            )
        })?;
    let grantor = listed.subject();
    if text(&g.claims, "sub").as_deref() != Some(grantor.as_str())
        || text(&g.claims, "iss").as_deref() != Some(grantor.as_str())
    {
        return Err(Error(
            "the grant says it is from a different identity than the one whose key signed it"
                .into(),
        ));
    }
    let (_, grant_exp) = check_times(
        &g.claims,
        now,
        policy.min_grant_secs,
        policy.max_grant_secs,
        "grant",
    )?;
    check_audience(&g.claims, "grant")?;
    let (Some(jti), Some(service), Some(scope)) = (
        text(&g.claims, "jti"),
        text(&g.claims, "service"),
        text(&g.claims, "scope"),
    ) else {
        return Err(Error(
            "the grant must carry jti, service and scope".into(),
        ));
    };
    let jwk = g
        .claims
        .pointer("/cnf/jwk")
        .ok_or_else(|| Error("the grant names no key for the service (no cnf.jwk)".into()))?;
    if text(jwk, "kty").as_deref() != Some("OKP")
        || text(jwk, "crv").as_deref() != Some("Ed25519")
    {
        return Err(Error("the grant names a key that is not Ed25519".into()));
    }
    let x = text(jwk, "x")
        .ok_or_else(|| Error("the grant names no key for the service (no cnf.jwk.x)".into()))?;

    // 2. The grant is still in force on this machine's list.
    let list = grants()?;
    let record = list
        .grants
        .iter()
        .find(|r| r.jti == jti)
        .ok_or_else(|| Error("the grant is not on this machine's list of grants".into()))?;
    if record.ended.is_some() {
        return Err(Error("the grant has been ended".into()));
    }
    if record.grantor != kid || record.service != service || record.x != x {
        return Err(Error(
            "the grant does not match what this machine's list says was granted".into(),
        ));
    }

    // 3. Verify the outer token's signature strictly with the granted service key.
    let service_key: [u8; 32] = URL_SAFE_NO_PAD
        .decode(&x)
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or_else(|| Error("the grant names a key of the wrong length".into()))?;
    VerifyingKey::from_bytes(&service_key)
        .map_err(|e| Error(format!("the service's key is not usable: {e}")))?
        .verify_strict(outer.signed.as_bytes(), &outer.signature)
        .map_err(|_| Error("the token is not signed by the service the grant names".into()))?;

    // 4. For the grantor, by the granted service.
    if text(&outer.claims, "sub").as_deref() != Some(grantor.as_str()) {
        return Err(Error(
            "the token is for someone other than the one who granted".into(),
        ));
    }
    if outer.claims.pointer("/act/sub").and_then(Value::as_str) != Some(service.as_str())
        || text(&outer.claims, "iss").as_deref() != Some(format!("service:{service}").as_str())
    {
        return Err(Error(
            "the token names a different service than the grant".into(),
        ));
    }

    // 5. Token dates, and never outliving the grant.
    let (_, exp) = check_times(&outer.claims, now, 0, policy.max_token_secs, "token")?;
    if exp > grant_exp {
        return Err(Error("the token outlives its grant".into()));
    }

    // 6. Audience.
    check_audience(&outer.claims, "token")?;

    let scopes = scope.split_whitespace().map(str::to_string).collect();
    Ok(build_caller(&listed, grantor, Some(service), Some(scopes), exp))
}
