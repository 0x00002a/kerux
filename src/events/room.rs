use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;

use crate::util::{mxid::RoomId, MatrixId};

use super::Redactable;

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum RoomVersion {
    #[serde(rename = "4")]
    V4,
    #[serde(other)]
    Unsupported,
}

/// m.room.create
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Create {
    pub creator: MatrixId,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room_version: Option<RoomVersion>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub predecessor: Option<PreviousRoom>,
    #[serde(flatten)]
    pub extra: HashMap<String, JsonValue>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PreviousRoom {
    pub room_id: RoomId,
    pub event_id: String,
}

impl Redactable for Create {
    fn redact(self) -> Self {
        Create {
            creator: self.creator,
            room_version: None,
            predecessor: None,
            extra: HashMap::new(),
        }
    }
}

/// m.room.join_rules
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct JoinRules {
    pub join_rule: JoinRule,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JoinRule {
    Public,
    Knock,
    Invite,
    Private,
}

impl Redactable for JoinRules {
    fn redact(self) -> Self {
        self
    }
}

/// m.room.history_visibility
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct HistoryVisibility {
    pub history_visibility: HistoryVisibilityType,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryVisibilityType {
    Invited,
    Joined,
    Shared,
    WorldReadable,
}

impl Redactable for HistoryVisibility {
    fn redact(self) -> Self {
        self
    }
}

/// m.room.guest_access
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GuestAccess {
    /// expected to only be None when redacted
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guest_access: Option<GuestAccessType>,
}

impl Redactable for GuestAccess {
    fn redact(self) -> Self {
        GuestAccess { guest_access: None }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GuestAccessType {
    CanJoin,
    Forbidden,
}

/// m.room.name
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Name {
    /// expected to only be None when redacted
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl Redactable for Name {
    fn redact(self) -> Self {
        Name { name: None }
    }
}

/// m.room.topic
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Topic {
    /// expected to only be None when redacted
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
}

impl Redactable for Topic {
    fn redact(self) -> Self {
        Topic { topic: None }
    }
}

/// m.room.power_levels
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PowerLevels {
    pub ban: Option<u32>,
    pub invite: Option<u32>,
    pub kick: Option<u32>,
    pub redact: Option<u32>,
    pub events: HashMap<String, u32>,
    pub events_default: Option<u32>,
    pub state_default: Option<u32>,
    pub users: HashMap<MatrixId, u32>,
    pub users_default: Option<u32>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notifications: Option<Notifications>,
}

impl PowerLevels {
    /// This function returns the effective power levels for when a room has no power levels event.
    /// The values are the same as when there is an event but the values are unspecified
    /// (i.e. `None`), with the exception that state_default is 0 and the creator of the room has
    /// power level 100.
    pub fn no_event_default_levels(room_creator: &MatrixId) -> Self {
        let mut users = HashMap::new();
        users.insert(room_creator.clone(), 100);
        PowerLevels {
            ban: Some(50),
            invite: Some(50),
            kick: Some(50),
            redact: Some(50),
            events: HashMap::new(),
            events_default: Some(0),
            state_default: Some(0),
            users,
            users_default: Some(0),
            notifications: None,
        }
    }

    pub fn ban(&self) -> u32 {
        self.ban.unwrap_or(50)
    }

    pub fn invite(&self) -> u32 {
        self.invite.unwrap_or(50)
    }

    pub fn kick(&self) -> u32 {
        self.kick.unwrap_or(50)
    }

    pub fn redact(&self) -> u32 {
        self.kick.unwrap_or(50)
    }

    pub fn events_default(&self) -> u32 {
        self.events_default.unwrap_or(0)
    }

    pub fn state_default(&self) -> u32 {
        self.state_default.unwrap_or(50)
    }

    pub fn users_default(&self) -> u32 {
        self.users_default.unwrap_or(0)
    }

    pub fn notifications(&self) -> Notifications {
        self.notifications.clone().unwrap_or_default()
    }

    pub fn get_user_level(&self, user_id: &MatrixId) -> u32 {
        self.users
            .get(user_id)
            .copied()
            .unwrap_or(self.users_default.unwrap_or(0))
    }

    pub fn get_event_level(&self, event_type: &str, is_state_event: bool) -> u32 {
        let default = if is_state_event {
            self.state_default.unwrap_or(50)
        } else {
            self.events_default.unwrap_or(0)
        };
        self.events.get(event_type).copied().unwrap_or(default)
    }
}

impl Redactable for PowerLevels {
    fn redact(self) -> Self {
        PowerLevels {
            ban: self.ban,
            invite: None,
            kick: self.kick,
            redact: self.redact,
            events: self.events,
            events_default: self.events_default,
            state_default: self.state_default,
            users: self.users,
            users_default: self.users_default,
            notifications: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Notifications {
    room: u32,
}

impl Default for PowerLevels {
    fn default() -> Self {
        PowerLevels {
            ban: Some(50),
            events: HashMap::new(),
            events_default: Some(0),
            invite: Some(50),
            kick: Some(50),
            redact: Some(50),
            state_default: Some(50),
            users: HashMap::new(),
            users_default: Some(50),
            notifications: Default::default(),
        }
    }
}

impl Default for Notifications {
    fn default() -> Self {
        Notifications { room: 50 }
    }
}

/// m.room.member
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Member {
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub displayname: Option<String>,
    pub membership: Membership,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_direct: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Membership {
    Invite,
    Join,
    Knock,
    Leave,
    Ban,
}

impl Redactable for Member {
    fn redact(self) -> Self {
        Member {
            avatar_url: None,
            displayname: None,
            membership: self.membership,
            is_direct: None,
        }
    }
}

#[derive(Debug)]
pub struct InvalidMembership(String);

impl std::str::FromStr for Membership {
    type Err = InvalidMembership;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ban" => Ok(Membership::Ban),
            "invite" => Ok(Membership::Invite),
            "join" => Ok(Membership::Join),
            "knock" => Ok(Membership::Knock),
            "leave" => Ok(Membership::Leave),
            x => Err(InvalidMembership(x.to_string())),
        }
    }
}

impl ToString for Membership {
    fn to_string(&self) -> String {
        use Membership::*;
        match self {
            Ban => "ban",
            Invite => "invite",
            Join => "join",
            Knock => "knock",
            Leave => "leave",
        }
        .to_string()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Redaction {
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

impl Redactable for Redaction {
    fn redact(self) -> Self {
        Redaction { reason: None }
    }
}
