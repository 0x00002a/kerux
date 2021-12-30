use async_trait::async_trait;
use displaydoc::Display;
use enum_extract::extract;
use std::convert::TryInto;
use serde_json::Value as JsonValue;

use crate::{error::{Error, ErrorKind}, events::{Event, EventContent, room::{self, Membership}, room_version::VersionedPdu}, state::StateResolver, storage::Storage, util::{MatrixId, MxidError}};

// TODO: builder pattern
pub struct NewEvent {
    pub event_content: EventContent,
    pub sender: MatrixId,
    pub state_key: Option<String>,
    pub redacts: Option<String>,
    pub unsigned: Option<JsonValue>,
}

#[derive(Debug, Display)]
pub enum AddEventError {
    /// A user tried to send an event to a room which they are not in.
    UserNotInRoom,
    /// A user tried to join a room from which they are banned.
    UserBanned,
    /// A user tried to join a private room to which they were not invited.
    UserNotInvited,
    /// A user tried to send an event to a room which does not exist.
    RoomNotFound,
    /// The user does not have the required power level to send this event.
    InsufficientPowerLevel,
    /// The event to be added was invalid.
    InvalidEvent(String),
}

#[async_trait]
pub trait StorageExt {
    async fn add_event(
        &self,
        room_id: &str,
        event: NewEvent,
        state_resolver: &StateResolver,
    ) -> Result<String, Error>;

    async fn get_sender_power_level(&self, room_id: &str, event_id: &str) -> Result<u32, Error>;

    async fn create_test_users(&self) -> Result<(), Error>;
}

#[async_trait]
impl<'a> StorageExt for dyn Storage + 'a {
    async fn add_event(
        &self,
        room_id: &str,
        event: NewEvent,
        state_resolver: &StateResolver,
    ) -> Result<String, Error> {
        // TODO: aaaaaaaaaaaaa
        let prev_events = &[];
        if let EventContent::Create(_) = event.event_content {
            panic!("wrong function");
        }
        let state = state_resolver.resolve(room_id, prev_events).await?;

        let mut auth_events = Vec::new();
        auth_events.push(state.get(("m.room.create", "")).unwrap().to_string());
        if let Some(power_levels_event) = state.get(("m.room.power_levels", "")) {
            auth_events.push(power_levels_event.to_string());
        }
        if let Some(member_event) = state.get(("m.room.member", event.sender.as_str())) {
            auth_events.push(member_event.to_string());
        }
        if let EventContent::Member(content) = &event.event_content {
            if let Some(target_member_event) = state.get(("m.room.member", &event.state_key.unwrap())) {
                auth_events.push(target_member_event.to_string());
            }
            if content.membership == Membership::Join
                || content.membership == Membership::Invite {
                if let Some(join_rules_event) = state.get(("m.room.join_rules", "")) {
                    auth_events.push(join_rules_event.to_string());
                }
            }
            // TODO: third party invites
        }
        // TODO: aaaaaaaaaaaaa
        Ok(String::new())
    }

    //TODO: check return type
    //TODO: should we handle users that aren't in the room
    async fn get_sender_power_level(&self, room_id: &str, event_id: &str) -> Result<u32, Error> {
        let event = self.get_pdu(room_id, event_id).await?.expect("event not found");
        let mut create_event_content = None;
        for auth_event_id in event.auth_events().iter() {
            let auth_event = self.get_pdu(room_id, auth_event_id).await?.expect("event not found");
            match auth_event.event_content() {
                EventContent::PowerLevels(levels) => {
                    return Ok(levels.get_user_level(event.sender()));
                },
                EventContent::Create(create) => {
                    create_event_content = Some(create.clone());
                },
                _ => {},
            }
        }

        // at this point there is no power levels event
        if *event.sender() == create_event_content.expect("event has no create in auth").creator {
            return Ok(100);
        } else {
            return Ok(0);
        }
    }



    async fn create_test_users(&self) -> Result<(), Error> {
        // all passwords are "password"
        self.create_user("alice",
            "$argon2i$v=19$m=4096,t=3,p=1$c2FsdHNhbHQ$llvUdqp69y2RB629dCuG42kR5y+Occ/ziKV5kn3rSOM"
        ).await?;
        self.create_user("bob",
            "$argon2i$v=19$m=4096,t=3,p=1$c2FsdHNhbHQ$llvUdqp69y2RB629dCuG42kR5y+Occ/ziKV5kn3rSOM"
        ).await?;
        self.create_user("carol",
            "$argon2i$v=19$m=4096,t=3,p=1$c2FsdHNhbHQ$llvUdqp69y2RB629dCuG42kR5y+Occ/ziKV5kn3rSOM"
        ).await?;
        Ok(())
    }
}

async fn validate_member_event(
    db: &dyn Storage,
    event: &Event,
    room_id: &str,
    power_levels: &room::PowerLevels,
) -> Result<(), Error> {
    let sender_membership = db.get_membership(&event.sender, room_id).await?;
    let affected_user = event.state_key.clone().ok_or_else(
        || AddEventError::InvalidEvent("no state key in m.room.member event".to_string())
    )?.try_into().map_err(|e: MxidError| AddEventError::InvalidEvent(e.to_string()))?;
    let prev_membership = db.get_membership(&affected_user, room_id).await?;

    // can't use extract because it's behind a reference how sad is that
    let new_member_content = match event.event_content {
        EventContent::Member(ref v) => v,
        _ => panic!("m.room.member not a member event"),
    };
    let new_membership = &new_member_content.membership;
    use room::Membership::*;
    match new_membership {
        Join => {
            if affected_user != event.sender {
                return Err(AddEventError::InvalidEvent(
                    "user tried to set someone else's membership to join".to_string()
                ).into());
            }
            match prev_membership {
                Some(Join) | Some(Invite) => {},
                Some(Ban) => return Err(AddEventError::UserBanned.into()),
                _ => {
                    let join_rules_event = db.get_state_event(room_id, "m.room.join_rules", "").await?;
                    let is_public = match join_rules_event {
                        Some(e) => {
                            let join_rules =
                                extract!(EventContent::JoinRules(_), e.event_content).unwrap();
                            join_rules.join_rule == room::JoinRule::Public
                        },
                        None => false,
                    };
                    if !is_public {
                        return Err(AddEventError::UserNotInvited.into());
                    }
                },
            }
        },
        Leave => {
            if sender_membership != Some(room::Membership::Join) {
                return Err(AddEventError::UserNotInRoom.into());
            }
            if event.state_key.as_deref() != Some(event.sender.as_str()) {
                // users can set own membership to leave, but setting others'
                // to leave is kicking and you need permission for that
                let user_level = power_levels.get_user_level(&event.sender);
                let kick_level = power_levels.kick();
                if user_level < kick_level {
                    return Err(AddEventError::InsufficientPowerLevel.into());
                }
            }
        },
        Ban => {
            if sender_membership != Some(room::Membership::Join) {
                return Err(AddEventError::UserNotInRoom.into());
            }
            let user_level = power_levels.get_user_level(&event.sender);
            let ban_level = power_levels.ban();
            if user_level < ban_level {
                return Err(AddEventError::InsufficientPowerLevel.into());
            }
        },
        Invite => {
            if sender_membership != Some(room::Membership::Join) {
                return Err(AddEventError::UserNotInRoom.into());
            }
            let user_level = power_levels.get_user_level(&event.sender);
            let invite_level = power_levels.invite();
            if user_level < invite_level {
                return Err(AddEventError::InsufficientPowerLevel.into());
            }
        },
        Knock => unimplemented!(),
    }
    Ok(())
}
