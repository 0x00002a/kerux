use displaydoc::Display;

use serde::{Deserialize, Serialize};
use std::{convert::TryFrom, str::FromStr};

use super::domain::Domain;

#[derive(Clone, Debug, Deserialize, Hash, PartialEq, Eq)]
#[serde(try_from = "String")]
pub struct MatrixId {
    domain: Domain,
    localpart: String,
}

#[derive(Debug, Display)]
pub enum MxidError {
    /// A Matrix ID can only be 255 characters long, including the '@', localpart, ':' and domain.
    TooLong,
    /// A Matrix ID can only have lowercase letters, numbers, and `-_.=/`.
    InvalidChar,
    /// A Matrix ID must begin with an '@'.
    NoLeadingAt,
    /// A Matrix ID must contain exactly one colon.
    WrongNumberOfColons,
    /// A Matrix ID must contain a valid domain name.
    InvalidDomain,
}

impl MatrixId {
    ///
    ///
    /// Unsafe because it doesn't validate localpart or domain
    unsafe fn new_unchecked(localpart: String, domain: Domain) -> Self {
        MatrixId { domain, localpart }
    }
    pub fn new(localpart: &str, domain: Domain) -> Result<Self, MxidError> {
        Self::validate_parts(localpart, &domain)?;
        // Safety: We just checked the precondition
        Ok(unsafe { Self::new_unchecked(localpart.to_owned(), domain) })
    }
    pub fn new_with_random_local(domain: Domain) -> Result<Self, MxidError> {
        let local = "random-username-implement-me";
        // Safety: Local will always be valid, domain was just checked
        Ok(unsafe { Self::new_unchecked(local.to_owned(), domain) })
    }

    pub fn localpart(&self) -> &str {
        &self.localpart
    }

    pub fn domain(&self) -> &Domain {
        &self.domain
    }

    /// Verifies that a localpart and domain could together form a valid Matrix ID.
    pub fn validate_parts(localpart: &str, domain: &Domain) -> Result<(), MxidError> {
        if localpart.contains(|c: char| {
            !c.is_ascii_lowercase()
                && !c.is_ascii_digit()
                && c != '-'
                && c != '_'
                && c != '.'
                && c != '='
                && c != '/'
        }) {
            return Err(MxidError::InvalidChar);
        }

        if localpart.len() + domain.as_str().len() + 2 > 255 {
            return Err(MxidError::TooLong);
        }

        Ok(())
    }

    /// Verifies that a `&str` forms a valid Matrix ID.
    pub fn validate_all(mxid: &str) -> Result<(Domain, &str), MxidError> {
        if !mxid.starts_with('@') {
            return Err(MxidError::NoLeadingAt);
        }
        let remaining: &str = &mxid[1..];
        let (localpart, domain) = {
            let mut iter = remaining.split(':');
            let localpart = iter.next().unwrap();
            let domain_parts = iter.collect::<Vec<_>>();
            if domain_parts.is_empty() {
                return Err(MxidError::WrongNumberOfColons);
            }
            let domain: Domain = domain_parts
                .join(":")
                .parse()
                .map_err(|_| MxidError::InvalidDomain)?;
            (localpart, domain)
        };
        Self::validate_parts(localpart, &domain)?;

        Ok((domain, localpart))
    }
}
impl std::fmt::Display for MatrixId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "@{local}:{domain}",
            domain = self.domain,
            local = self.localpart
        ))
    }
}

impl FromStr for MatrixId {
    type Err = MxidError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (domain, localpart) = MatrixId::validate_all(value)?;
        Ok(unsafe { Self::new_unchecked(localpart.to_owned(), domain) })
    }
}
impl TryFrom<&str> for MatrixId {
    type Error = MxidError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl TryFrom<String> for MatrixId {
    type Error = MxidError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}
impl Serialize for MatrixId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.to_string().serialize(serializer)
    }
}
#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use crate::util::{domain::Domain, MatrixId};

    #[test]
    fn matrix_id_serializes_correctly() {
        assert_eq!(
            serde_json::to_string(
                &MatrixId::new("test", Domain::new("local".to_owned()).unwrap()).unwrap()
            )
            .as_deref()
            .unwrap(),
            "\"@test:local\""
        );
    }
    #[test]
    fn matrix_id_parsing_preserves_port_in_domain() {
        let id = MatrixId::from_str("@name:test:8000").unwrap();
        assert_eq!(id.localpart(), "name");
        assert_eq!(id.domain().as_str(), "test:8000");
    }
}
