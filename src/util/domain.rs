use std::{convert::TryFrom, str::FromStr};

use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
lazy_static! {
    static ref SERVER_NAME_REGEX: Regex =
        Regex::new(include_str!("./mxid_server_name.regex")).unwrap();
}

/// Matrix server domain
#[repr(transparent)]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Hash, PartialOrd, Ord)]
#[serde(try_from = "String")]
pub struct Domain {
    url: String,
}

impl Domain {
    pub fn new(url: String) -> Option<Self> {
        if !Self::is_valid(&url) {
            None
        } else {
            Some(Self { url })
        }
    }
    pub fn as_str(&self) -> &str {
        self.url.as_str()
    }
    pub fn is_valid(url: &str) -> bool {
        SERVER_NAME_REGEX.is_match(url)
    }
}
impl std::fmt::Display for Domain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.url)
    }
}
#[derive(Debug, PartialEq, Eq)]
pub struct InvalidDomainError {}
impl std::error::Error for InvalidDomainError {}
impl std::fmt::Display for InvalidDomainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("invalid domain string")
    }
}
impl Serialize for Domain {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.as_str().serialize(serializer)
    }
}

impl TryFrom<String> for Domain {
    type Error = InvalidDomainError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}
impl FromStr for Domain {
    type Err = InvalidDomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if !Self::is_valid(s) {
            Err(InvalidDomainError {})
        } else {
            Ok(Self { url: s.to_owned() })
        }
    }
}
#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use crate::util::domain::Domain;

    fn check_parse(domain: &str) {
        assert_eq!(
            domain.parse(),
            Ok(Domain {
                url: domain.to_owned()
            })
        );
    }

    #[test]
    fn domain_serializes_like_a_string() {
        assert_eq!(
            serde_json::to_value(Domain::from_str("hello").unwrap()).unwrap(),
            serde_json::json!("hello")
        );
    }

    #[test]
    fn lan_domain_is_valid() {
        check_parse("something.lan")
    }
    #[test]
    fn google_dot_com_is_valid() {
        check_parse("google.com");
        check_parse("www.google.com");
    }

    #[test]
    fn domain_with_port_is_valid() {
        check_parse("thingy.com:5442");
    }
}
