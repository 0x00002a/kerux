use displaydoc::Display;

use serde::{Deserialize, Serialize};
use std::{convert::TryFrom, str::FromStr};

use super::domain::Domain;

/// Generic type for a matrix idenifier
///
/// See `MatrixId` and `RoomId`
#[derive(Clone, Debug, Deserialize, Hash, PartialEq, Eq, PartialOrd, Ord)]
#[serde(try_from = "String")]
pub struct Id<const PREFIX: char> {
    domain: Domain,
    localpart: String,
}
/// Identifier for a user
pub type MatrixId = Id<'@'>;
/// Identifier for a room
pub type RoomId = Id<'!'>;

#[derive(Debug, Display)]
pub enum MxidError {
    /// A Matrix ID can only be 255 characters long, including the '@', localpart, ':' and domain.
    TooLong,
    /// A Matrix ID can only have lowercase letters, numbers, and `-_.=/`.
    InvalidChar,
    /// An ID must begin {0}
    NoLeadingChar(char),
    /// A Matrix ID must contain exactly one colon.
    WrongNumberOfColons,
    /// A Matrix ID must contain a valid domain name.
    InvalidDomain,
}

impl<const PREFIX: char> Id<PREFIX> {
    ///
    ///
    /// Unsafe because it doesn't validate localpart or domain
    unsafe fn new_unchecked(localpart: String, domain: Domain) -> Self {
        Self { domain, localpart }
    }
    pub fn new(localpart: &str, domain: Domain) -> Result<Self, MxidError> {
        Self::validate_parts(localpart, &domain)?;
        // Safety: We just checked the precondition
        Ok(unsafe { Self::new_unchecked(localpart.to_owned(), domain) })
    }

    pub fn localpart(&self) -> &str {
        &self.localpart
    }

    pub fn domain(&self) -> &Domain {
        &self.domain
    }
    /// Length when displayed as a string
    ///
    /// Includes prefix and seperators
    fn str_len(&self) -> usize {
        self.domain.as_str().len() + self.localpart.len() + 1 + PREFIX.len_utf8()
    }
    pub fn is_matrix_id() -> bool {
        PREFIX == '@'
    }

    /// Verifies that a localpart and domain could together form a valid Matrix ID.
    pub fn validate_parts(localpart: &str, domain: &Domain) -> Result<(), MxidError> {
        if Self::is_matrix_id()
            && localpart.contains(|c: char| {
                !c.is_ascii_lowercase()
                    && !c.is_ascii_digit()
                    && c != '-'
                    && c != '_'
                    && c != '.'
                    && c != '='
                    && c != '/'
            })
        {
            return Err(MxidError::InvalidChar);
        }

        if localpart.len() + domain.as_str().len() + 1 + PREFIX.len_utf8() > 255 {
            return Err(MxidError::TooLong);
        }

        Ok(())
    }

    /// Verifies that a `&str` forms a valid Matrix ID.
    pub fn validate_all(mxid: &str) -> Result<(Domain, &str), MxidError> {
        if !mxid.starts_with(PREFIX) {
            return Err(MxidError::NoLeadingChar(PREFIX));
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
    fn new_len_checked(local: String, domain: Domain) -> Result<Self, MxidError> {
        if domain.as_str().len() + 2 + local.len() > 255 {
            Err(MxidError::TooLong)
        } else {
            // Safety: We just checked the requirements
            Ok(unsafe { Self::new_unchecked(local, domain) })
        }
    }
}
impl Id<'@'> {
    pub fn new_with_random_local(domain: Domain) -> Result<Self, MxidError> {
        let local = "todo-impl-me";
        Self::new_len_checked(local.to_owned(), domain)
    }
}
impl Id<'!'> {
    pub fn new_with_random_local(domain: Domain) -> Result<Self, MxidError> {
        let local = format!("{:016X}", rand::random::<i64>());
        Self::new_len_checked(local, domain)
    }
}

impl<const P: char> std::fmt::Display for Id<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "{P}{local}:{domain}",
            domain = self.domain,
            local = self.localpart
        ))
    }
}

impl<const P: char> FromStr for Id<P> {
    type Err = MxidError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (domain, localpart) = Self::validate_all(value)?;
        Ok(unsafe { Self::new_unchecked(localpart.to_owned(), domain) })
    }
}
impl<const P: char> TryFrom<&str> for Id<P> {
    type Error = MxidError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl<const P: char> TryFrom<String> for Id<P> {
    type Error = MxidError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}
impl<const P: char> Serialize for Id<P> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.to_string().serialize(serializer)
    }
}
impl<const P: char> PartialEq<&str> for Id<P> {
    fn eq(&self, other: &&str) -> bool {
        if other.len() != self.str_len() || !other.starts_with(P) {
            return false;
        }
        let Some((local, domain)) = other.split_once(':') else { return false };
        self.localpart == local[1..] && self.domain.as_str() == domain
    }
}
#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use crate::{
        assert_err, assert_ok,
        util::{
            domain::Domain,
            mxid::{Id, RoomId},
            MatrixId,
        },
    };

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
    #[test]
    fn id_requires_first_character_to_match_prefix() {
        let rs = Id::<'a'>::from_str("aname:test");
        assert_ok!(rs);
        let rs = Id::<'a'>::from_str("bname:test");
        assert_err!(rs);
    }

    #[test]
    fn id_prints_prefix_as_first_char_with_display() {
        let id = Id::<'a'> {
            domain: Domain::new("test".to_owned()).unwrap(),
            localpart: "hello".to_owned(),
        };
        assert!(id.to_string().starts_with('a'));
    }

    #[test]
    fn id_can_be_compared_to_a_string() {
        let id = MatrixId::new("a", "b".parse().unwrap()).unwrap();
        assert_eq!(id, "@a:b");
        assert_ne!(id, "@ab", "id's do not match invalid strings");
    }

    #[test]
    fn room_id_can_be_parsed_from_prefix_exclamation() {
        let id = RoomId::from_str("!a:b");
        assert_ok!(id);
    }
    #[test]
    fn random_room_id_is_valid() {
        let id = RoomId::new_with_random_local("b".parse().unwrap()).unwrap();
        assert_ok!(RoomId::from_str(&id.to_string()));
    }
}
