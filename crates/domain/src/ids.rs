//! Identifier newtypes: [`TenantId`], [`Id`] and [`Version`].
//!
//! Every type here checks its invariant in the constructor and keeps the field private, so
//! `TenantId(Uuid::nil())` cannot be built anywhere in the workspace. That matters because
//! nil is a valid Postgres uuid: a nil tenant id matches nothing and reads as an empty
//! tenant instead of an error.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

/// Why an identifier could not be built.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum IdError {
    /// A uuid with every bit zero. Valid Postgres, never a real row.
    #[error("a uuid must not be nil")]
    Nil,
    /// Anything that is not the bare, hyphenated 36-character form.
    #[error("not a bare hyphenated uuid: {0}")]
    Parse(String),
    /// `version` starts at 1; 0 is not a version any row can have (`DEFAULT 1`).
    #[error("a version starts at 1")]
    ZeroVersion,
}

/// A tenant (customer) id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TenantId(Uuid);

impl TenantId {
    /// A fresh uuid v7 (time-ordered, the estate's primary-key rule).
    // `Default` is deliberately NOT implemented: a silently created tenant id would look
    // like a real tenant and hide the missing value. Callers spell `TenantId::new()`.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    /// The bare, hyphenated 36-character form only. Braced and urn forms are rejected
    /// rather than normalised: one spelling in, one spelling out.
    pub fn parse(raw: &str) -> Result<Self, IdError> {
        if raw.len() != 36 {
            return Err(IdError::Parse(format!(
                "expected 36 characters, got {}",
                raw.len()
            )));
        }
        let uuid = Uuid::parse_str(raw).map_err(|e| IdError::Parse(e.to_string()))?;
        Self::from_uuid(uuid)
    }

    /// The one checked conversion from a raw uuid.
    pub fn from_uuid(uuid: Uuid) -> Result<Self, IdError> {
        if uuid.is_nil() {
            Err(IdError::Nil)
        } else {
            Ok(Self(uuid))
        }
    }

    /// The database value. Binding goes through this, never through a `String`.
    pub fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl fmt::Display for TenantId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for TenantId {
    type Err = IdError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl Serialize for TenantId {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for TenantId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(d)?;
        Self::parse(&raw).map_err(serde::de::Error::custom)
    }
}

/// Any non-tenant row id (a uuid v7 primary key).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Id(Uuid);

impl Id {
    /// A fresh uuid v7.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    /// Parse the bare, hyphenated form; reject nil.
    pub fn parse(raw: &str) -> Result<Self, IdError> {
        if raw.len() != 36 {
            return Err(IdError::Parse(format!(
                "expected 36 characters, got {}",
                raw.len()
            )));
        }
        let uuid = Uuid::parse_str(raw).map_err(|e| IdError::Parse(e.to_string()))?;
        Self::from_uuid(uuid)
    }

    /// The one checked conversion from a raw uuid.
    pub fn from_uuid(uuid: Uuid) -> Result<Self, IdError> {
        if uuid.is_nil() {
            Err(IdError::Nil)
        } else {
            Ok(Self(uuid))
        }
    }

    /// The database value.
    pub fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for Id {
    type Err = IdError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl Serialize for Id {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for Id {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(d)?;
        Self::parse(&raw).map_err(serde::de::Error::custom)
    }
}

/// An optimistic-concurrency version (`bigint NOT NULL DEFAULT 1`).
///
/// `0` is rejected: no row ever has it, so a decoded `0` is a bug at the boundary, not a
/// stale write. The `If-Match` header carries [`Version::etag`]'s quoted form.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Version(u64);

impl Version {
    /// The version a newly created row has.
    pub const INITIAL: Version = Version(1);

    /// Construct; `0` is not a version.
    pub fn new(value: u64) -> Result<Self, IdError> {
        if value == 0 {
            Err(IdError::ZeroVersion)
        } else {
            Ok(Self(value))
        }
    }

    /// The numeric value (for `WHERE version = $n` and `ETag`).
    pub fn get(self) -> u64 {
        self.0
    }

    /// The next version of a row that is being updated.
    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }

    /// The `ETag` / `If-Match` representation: the number in double quotes.
    pub fn etag(self) -> String {
        format!("\"{}\"", self.0)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for Version {
    type Err = IdError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let raw = s.trim().trim_matches('"');
        let value = raw
            .parse::<u64>()
            .map_err(|e: std::num::ParseIntError| IdError::Parse(e.to_string()))?;
        Self::new(value)
    }
}

impl Serialize for Version {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u64(self.0)
    }
}

impl<'de> Deserialize<'de> for Version {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = u64::deserialize(d)?;
        Self::new(raw).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = "0195f48b-d000-7000-8000-000000000099";

    #[test]
    fn nil_is_rejected_by_every_constructor() {
        assert_eq!(TenantId::from_uuid(Uuid::nil()).unwrap_err(), IdError::Nil);
        assert_eq!(Id::from_uuid(Uuid::nil()).unwrap_err(), IdError::Nil);
        assert_eq!(
            TenantId::parse("00000000-0000-0000-0000-000000000000").unwrap_err(),
            IdError::Nil
        );
        assert_eq!(
            Id::parse("00000000-0000-0000-0000-000000000000").unwrap_err(),
            IdError::Nil
        );
    }

    #[test]
    fn parse_accepts_only_the_bare_hyphenated_form() {
        assert_eq!(TenantId::parse(VALID).unwrap().as_uuid().to_string(), VALID);
        for raw in [
            "0195f48bd00070008000000000000099",              // no hyphens
            "{0195f48b-d000-7000-8000-000000000099}",        // braced
            "urn:uuid:0195f48b-d000-7000-8000-000000000099", // urn
            "nope",
            "",
            " 0195f48b-d000-7000-8000-000000000099",
            "0195f48b-d000-7000-8000-000000000099 ",
        ] {
            assert!(TenantId::parse(raw).is_err(), "{raw:?} must be rejected");
            assert!(Id::parse(raw).is_err(), "{raw:?} must be rejected");
        }
    }

    #[test]
    fn new_ids_are_non_nil_v7() {
        let t = TenantId::new();
        assert!(!t.as_uuid().is_nil());
        assert_eq!(t.as_uuid().get_version_num(), 7);
        let i = Id::new();
        assert!(!i.as_uuid().is_nil());
        assert_eq!(i.as_uuid().get_version_num(), 7);
    }

    #[test]
    fn version_zero_does_not_exist_and_etag_is_quoted() {
        assert_eq!(Version::new(0).unwrap_err(), IdError::ZeroVersion);
        assert_eq!(Version::INITIAL.get(), 1);
        assert_eq!(Version::new(7).unwrap().etag(), "\"7\"");
        assert_eq!(Version::new(7).unwrap().next(), Version::new(8).unwrap());
        assert_eq!(Version::INITIAL.next(), Version::new(2).unwrap());
    }

    #[test]
    fn serde_round_trips_and_rejects_invalid_values() {
        let t: TenantId = serde_json::from_str(&format!("\"{VALID}\"")).unwrap();
        assert_eq!(serde_json::to_string(&t).unwrap(), format!("\"{VALID}\""));
        assert!(serde_json::from_str::<TenantId>("\"nope\"").is_err());

        let v: Version = serde_json::from_str("3").unwrap();
        assert_eq!(v.get(), 3);
        assert!(serde_json::from_str::<Version>("0").is_err());
    }
}
