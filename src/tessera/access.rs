//! Named-graph and service-scope access control (`subtexture 0.6.0`).
//!
//! Once [`super::verify`] has answered *who is asking*, this module answers
//! *may they read this named graph* (and, when a service is acting on their
//! behalf, *does its grant carry the required scope*).
//!
//! Every named graph in a Subtexture store describes itself in the store's
//! unnamed metadata graph (`2026_11_19_NAMED_GRAPH_DESCRIPTIONS_SPEC.md`,
//! Section 12):
//!
//! - `subtexture:owner`: the Soul IRI of the graph's owner; always allowed.
//! - `subtexture:visibility`: `"private"` (default when omitted), `"shared"`,
//!   or `"public"`.
//! - `subtexture:readableBy`: when `"shared"`, zero or more Soul IRIs
//!   (`https://repolex.ai/7r1pl3f0rc3/w4r3z/soul/Soul/w4r3z`) or squad IRIs
//!   (`https://repolex.ai/7r1pl3f0rc3`).
//!
//! In addition, the machine's recorded [`Owner`] (`~/.lex/identity/owner.json`)
//! may read every graph on the machine.

use super::keyring::Owner;
use super::verify::{www_authenticate_insufficient_scope, Caller, AUDIENCE};
use super::{Error, Result};
use serde::{Deserialize, Serialize};

/// Access policy recorded in `subtexture:visibility` on a named graph.
/// Defaults to [`Visibility::Private`] when omitted on the graph description.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    /// Only the graph's `owner` (and the machine's `Owner`) may read. Default
    /// when `subtexture:visibility` is absent.
    #[default]
    Private,
    /// Readable by `owner`, the machine's `Owner`, and any Soul or squad listed
    /// in `subtexture:readableBy`.
    Shared,
    /// Readable by anyone, including an unidentified caller (`None`).
    Public,
}

impl Visibility {
    /// Parse an optional `subtexture:visibility` literal. `None` becomes
    /// [`Visibility::Private`]; an unknown string returns `Err` so a misspelled
    /// value never fails open or silently changes policy.
    pub fn parse(value: Option<&str>) -> Result<Self> {
        match value {
            None | Some("private") => Ok(Visibility::Private),
            Some("shared") => Ok(Visibility::Shared),
            Some("public") => Ok(Visibility::Public),
            Some(other) => Err(Error(format!(
                "unknown visibility {other:?}; expected \"private\", \"shared\" or \"public\""
            ))),
        }
    }

    /// Canonical RDF literal (`"private"`, `"shared"`, or `"public"`).
    pub fn as_str(self) -> &'static str {
        match self {
            Visibility::Private => "private",
            Visibility::Shared => "shared",
            Visibility::Public => "public",
        }
    }
}

/// Access metadata of one named graph, read from its description in the store's
/// unnamed graph.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GraphAccess<'a> {
    /// Value of `subtexture:owner` on the graph, if present.
    pub owner: Option<&'a str>,
    /// Value of `subtexture:visibility` (`Visibility::Private` when omitted).
    pub visibility: Visibility,
    /// Values of `subtexture:readableBy` (Soul IRIs and/or squad IRIs).
    pub readable_by: &'a [&'a str],
}

/// Why access to a graph or route was refused, distinguishing HTTP `401` from
/// `403`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Denial {
    /// `401 Unauthorized`: the caller sent no token (`None`), and the graph is
    /// not `public`.
    Unauthenticated(String),
    /// `403 Forbidden`: the caller is identified, and neither owns nor is
    /// granted access to this graph.
    Forbidden(String),
    /// `403 Forbidden`: the caller is acting through a service whose grant does
    /// not carry the required scope word.
    InsufficientScope { required: String, reason: String },
}

impl Denial {
    /// Corresponding HTTP status code (`401` for `Unauthenticated`, `403` for
    /// `Forbidden` and `InsufficientScope`).
    pub fn http_status(&self) -> u16 {
        match self {
            Denial::Unauthenticated(_) => 401,
            Denial::Forbidden(_) | Denial::InsufficientScope { .. } => 403,
        }
    }

    /// Plain-language reason suitable for a JSON error body.
    pub fn reason(&self) -> &str {
        match self {
            Denial::Unauthenticated(msg) | Denial::Forbidden(msg) => msg,
            Denial::InsufficientScope { reason, .. } => reason,
        }
    }

    /// RFC 6750 `WWW-Authenticate` response header value, when applicable (`401`
    /// prompt for a Bearer token, or `403` `insufficient_scope`).
    pub fn www_authenticate(&self) -> Option<String> {
        match self {
            Denial::Unauthenticated(_) => Some(format!("Bearer realm=\"{AUDIENCE}\"")),
            Denial::InsufficientScope { required, .. } => {
                Some(www_authenticate_insufficient_scope(required))
            }
            Denial::Forbidden(_) => None,
        }
    }
}

impl std::fmt::Display for Denial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.reason())
    }
}

impl std::error::Error for Denial {}

/// Require that an identified caller's token permits `required_scope` (such as
/// `"syrinx:read"`, `"pan:read"`, or `"pan:write"`).
///
/// An identity acting for itself (`caller.scope == None`) always passes; a
/// service acting under a grant passes only if its grant lists `required_scope`.
pub fn require_scope(caller: &Caller, required_scope: &str) -> std::result::Result<(), Denial> {
    if caller.allows_scope(required_scope) {
        return Ok(());
    }
    let actor = caller.actor.as_deref().unwrap_or("service");
    Err(Denial::InsufficientScope {
        required: required_scope.to_string(),
        reason: format!(
            "the grant for {actor} does not include the required scope {required_scope:?}"
        ),
    })
}

/// Check whether `caller` may read a named graph described by `graph`, given
/// the machine's optional `machine_owner` (`owner.json`).
///
/// Rules (`subtexture 0.6.0`, Spec Section 12.2):
///
/// 1. `Visibility::Public`: allowed for everyone, including `caller == None`.
/// 2. `caller == None` on a `Private` or `Shared` graph: refused with
///    [`Denial::Unauthenticated`] (`401`).
/// 3. Machine owner (`machine_owner.matches(caller)`): allowed.
/// 4. Graph owner (`graph.owner == Some(caller.subject)`): allowed.
/// 5. `Visibility::Shared`: allowed if `graph.readable_by` contains
///    `caller.subject` or the caller's squad IRI (`https://repolex.ai/{squad}`).
/// 6. Everything else: refused with [`Denial::Forbidden`] (`403`).
pub fn check_read(
    caller: Option<&Caller>,
    graph: &GraphAccess<'_>,
    machine_owner: Option<&Owner>,
) -> std::result::Result<(), Denial> {
    if graph.visibility == Visibility::Public {
        return Ok(());
    }
    let Some(caller) = caller else {
        return Err(Denial::Unauthenticated(
            "this graph is not public; send Authorization: Bearer <token>".into(),
        ));
    };
    if machine_owner.is_some_and(|mo| mo.matches(caller)) {
        return Ok(());
    }
    if graph.owner == Some(caller.subject.as_str()) {
        return Ok(());
    }
    if graph.visibility == Visibility::Shared {
        let squad_iri = caller.squad_iri();
        let listed = graph.readable_by.iter().any(|entry| {
            *entry == caller.subject
                || squad_iri
                    .as_deref()
                    .is_some_and(|sq| entry.trim_end_matches('/') == sq)
        });
        if listed {
            return Ok(());
        }
    }
    Err(Denial::Forbidden(format!(
        "{} is not permitted to read this {} graph",
        caller.subject,
        graph.visibility.as_str()
    )))
}
