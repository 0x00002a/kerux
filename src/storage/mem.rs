use async_trait::async_trait;
use serde_json::Value as JsonValue;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{
    broadcast::{channel, Sender},
    RwLock,
};
use uuid::Uuid;

use crate::{
    error::{Error, ErrorKind},
    events::{ephemeral::Typing, pdu::StoredPdu, EventContent},
    storage::{Batch, EventQuery, QueryType, Storage, StorageManager, UserProfile},
    util::{mxid::RoomId, MatrixId},
};

use super::EventQueryResult;

struct MemStorage {
    rooms: HashMap<RoomId, Room>,
    users: Vec<User>,
    access_tokens: HashMap<Uuid, String>,
    batches: HashMap<String, Batch>,
    txn_ids: HashMap<Uuid, HashSet<String>>,
}

#[derive(Debug)]
struct Room {
    events: Vec<StoredPdu>,
    ephemeral: HashMap<String, JsonValue>,
    typing: HashMap<MatrixId, Instant>,
    notify_send: Sender<()>,
}

#[derive(Debug)]
struct User {
    username: String,
    password_hash: String,
    profile: UserProfile,
    account_data: HashMap<String, JsonValue>,
}

pub struct MemStorageManager {
    storage: Arc<RwLock<MemStorage>>,
}

pub struct MemStorageHandle {
    inner: Arc<RwLock<MemStorage>>,
}

impl Room {
    fn new() -> Self {
        Room {
            events: Vec::new(),
            ephemeral: HashMap::new(),
            typing: Default::default(),
            notify_send: channel(1).0,
        }
    }
}

impl MemStorageManager {
    pub fn new() -> Self {
        MemStorageManager {
            storage: Arc::new(RwLock::new(MemStorage {
                rooms: HashMap::new(),
                users: Vec::new(),
                access_tokens: HashMap::new(),
                batches: HashMap::new(),
                txn_ids: HashMap::new(),
            })),
        }
    }
}

#[async_trait]
impl StorageManager for MemStorageManager {
    async fn get_handle(&self) -> Result<Box<dyn Storage>, Error> {
        Ok(Box::new(MemStorageHandle {
            inner: Arc::clone(&self.storage),
        }))
    }
}

#[async_trait]
impl Storage for MemStorageHandle {
    async fn overwrite_profile(&self, username: &str, profile: UserProfile) -> Result<(), Error> {
        let mut db = self.inner.write().await;
        let pos = db
            .users
            .iter()
            .position(|u| u.username == username)
            .ok_or(Error::from(ErrorKind::UserNotFound))?;
        db.users[pos].profile = profile;
        Ok(())
    }
    async fn create_user(&self, username: &str, password: &str) -> Result<(), Error> {
        let salt: [u8; 16] = rand::random();
        let password_hash = argon2::hash_encoded(password.as_bytes(), &salt, &Default::default())?;
        let mut db = self.inner.write().await;
        if db.users.iter().any(|u| u.username == username) {
            return Err(ErrorKind::UsernameTaken.into());
        }
        db.users.push(User {
            username: username.to_string(),
            password_hash: password_hash.to_string(),
            profile: UserProfile {
                avatar_url: None,
                displayname: None,
                status: None,
            },
            account_data: HashMap::new(),
        });
        Ok(())
    }

    async fn verify_password(&self, username: &str, password: &str) -> Result<bool, Error> {
        let db = self.inner.read().await;
        let user = db.users.iter().find(|u| u.username == username);
        if let Some(user) = user {
            match argon2::verify_encoded(&user.password_hash, password.as_bytes()) {
                Ok(true) => Ok(true),
                Ok(false) => Ok(false),
                Err(_) => Ok(false),
            }
        } else {
            Ok(false)
        }
    }

    async fn create_access_token(&self, username: &str, _device_id: &str) -> Result<Uuid, Error> {
        let mut db = self.inner.write().await;
        let token = Uuid::new_v4();
        if !db.users.iter().any(|u| u.username == username) {
            return Err(ErrorKind::UserNotFound.into());
        }
        db.access_tokens.insert(token, username.to_string());
        Ok(token)
    }

    async fn delete_access_token(&self, token: Uuid) -> Result<(), Error> {
        let mut db = self.inner.write().await;
        db.access_tokens.remove(&token);
        Ok(())
    }

    async fn delete_all_access_tokens(&self, token: Uuid) -> Result<(), Error> {
        let mut db = self.inner.write().await;
        let username = match db.access_tokens.get(&token) {
            Some(v) => v.clone(),
            None => return Ok(()),
        };
        db.access_tokens.retain(|_token, name| *name != username);
        Ok(())
    }

    async fn try_auth(&self, token: Uuid) -> Result<Option<String>, Error> {
        let db = self.inner.read().await;
        Ok(db.access_tokens.get(&token).cloned())
    }

    async fn record_txn(&self, token: Uuid, txn_id: String) -> Result<bool, Error> {
        let mut db = self.inner.write().await;
        let set = db.txn_ids.entry(token).or_insert_with(HashSet::new);
        Ok(set.insert(txn_id))
    }

    async fn get_profile(&self, username: &str) -> Result<Option<UserProfile>, Error> {
        let db = self.inner.read().await;
        Ok(db
            .users
            .iter()
            .find(|u| u.username == username)
            .map(|u| u.profile.clone()))
    }

    async fn set_avatar_url(&self, username: &str, avatar_url: &str) -> Result<(), Error> {
        let mut db = self.inner.write().await;
        let user = db
            .users
            .iter_mut()
            .find(|u| u.username == username)
            .ok_or(ErrorKind::UserNotFound)?;
        user.profile.avatar_url = Some(avatar_url.to_string());
        Ok(())
    }

    async fn set_display_name(&self, username: &str, display_name: &str) -> Result<(), Error> {
        let mut db = self.inner.write().await;
        let user = db
            .users
            .iter_mut()
            .find(|u| u.username == username)
            .ok_or(ErrorKind::UserNotFound)?;
        user.profile.displayname = Some(display_name.to_string());
        Ok(())
    }

    async fn add_pdus(&self, pdus: &[StoredPdu]) -> Result<(), Error> {
        let mut db = self.inner.write().await;
        for pdu in pdus {
            if let EventContent::Create(_) = pdu.event_content() {
                db.rooms.insert(pdu.room_id().to_owned(), Room::new());
            }
            db.rooms
                .get_mut(pdu.room_id())
                .ok_or(ErrorKind::RoomNotFound)?
                .events
                .push(pdu.clone());
        }
        Ok(())
    }

    async fn get_prev_events(&self, room_id: &RoomId) -> Result<(Vec<String>, i64), Error> {
        let db = self.inner.read().await;
        let room = db.rooms.get(room_id).ok_or(ErrorKind::RoomNotFound)?;
        let mut prev_events = room.events.clone();
        for event in room.events.iter() {
            for prev in event.prev_events() {
                prev_events.retain(|pdu| pdu.event_id() != *prev);
            }
        }
        let event_ids = prev_events
            .iter()
            .map(|pdu| pdu.event_id())
            .collect::<Vec<_>>();
        let max_depth = prev_events
            .iter()
            .map(|pdu| pdu.depth())
            .max()
            .unwrap_or(-1); // no events in room
        Ok((event_ids, max_depth))
    }

    async fn query_pdus<'a>(
        &self,
        query: EventQuery<'a>,
        wait: bool,
    ) -> Result<EventQueryResult<StoredPdu>, Error> {
        let mut ret = Vec::new();
        let (mut from, mut to) = match query.query_type {
            QueryType::Timeline { from, to } => (from, to),
            QueryType::State { at, .. } => (0, at),
        };

        let db = self.inner.read().await;
        let room = db.rooms.get(query.room_id).ok_or(ErrorKind::RoomNotFound)?;
        if to.is_none() {
            to = Some(room.events.len() - 1);
        }

        if let Some(range) = room.events.get(from..=to.unwrap()) {
            ret.extend(
                range
                    .iter()
                    .filter(|pdu| query.matches(pdu.inner()))
                    .cloned(),
            );
        }

        if wait && ret.is_empty() && query.query_type.is_timeline() {
            let mut recv = room.notify_send.subscribe();
            // Release locks; we are about to wait for new events to come in, and they can't if we've
            // locked the db
            drop(db);
            // This returns a result, but one of the possible errors is "there are multiple
            // events" which is what we're waiting for anyway, and the other is "send half has
            // been dropped" which would mean we have bigger problems than this one query
            let _ = recv.recv().await;
            from = to.unwrap();
            to = None;
        } else {
            return Ok(EventQueryResult {
                events: ret,
                timeline_end: to.unwrap(),
            });
        }

        // same again
        let db = self.inner.read().await;
        let room = db.rooms.get(query.room_id).ok_or(ErrorKind::RoomNotFound)?;
        if to.is_none() {
            to = Some(room.events.len() - 1);
        }

        if let Some(range) = room.events.get(from..=to.unwrap()) {
            ret.extend(
                range
                    .iter()
                    .filter(|pdu| query.matches(pdu.inner()))
                    .cloned(),
            );
        }

        if query.query_type.is_state() {
            ret.reverse();
            /*            let mut seen = HashSet::new();
            // remove pdus that are older than another pdu with the same state key
            ret.retain(|pdu| {
                seen.insert(pdu.state_key().to_string().unwrap())
            });*/
            ret.reverse();
        }

        Ok(EventQueryResult {
            events: ret,
            timeline_end: to.unwrap(),
        })
    }

    async fn get_rooms(&self) -> Result<Vec<RoomId>, Error> {
        let db = self.inner.read().await;
        Ok(db.rooms.keys().cloned().collect())
    }

    async fn get_pdu(&self, room_id: &RoomId, event_id: &str) -> Result<Option<StoredPdu>, Error> {
        let db = self.inner.read().await;
        let event = db
            .rooms
            .get(room_id)
            .and_then(|r| r.events.iter().find(|e| e.event_id() == event_id))
            .cloned();
        Ok(event)
    }

    async fn get_all_ephemeral(
        &self,
        room_id: &RoomId,
    ) -> Result<HashMap<String, JsonValue>, Error> {
        let db = self.inner.read().await;
        let room = db.rooms.get(room_id).ok_or(ErrorKind::RoomNotFound)?;
        let mut ephemeral = room.ephemeral.clone();

        let now = Instant::now();
        let mut typing = Typing::default();
        for (mxid, _) in room.typing.iter().filter(|(_, timeout)| **timeout > now) {
            typing.user_ids.insert(mxid.clone());
        }
        ephemeral.insert(
            String::from("m.typing"),
            serde_json::to_value(typing).unwrap(),
        );
        Ok(ephemeral)
    }

    async fn get_ephemeral(
        &self,
        room_id: &RoomId,
        event_type: &str,
    ) -> Result<Option<JsonValue>, Error> {
        let db = self.inner.read().await;
        let room = db.rooms.get(room_id).ok_or(ErrorKind::RoomNotFound)?;
        if event_type == "m.typing" {
            let now = Instant::now();
            let mut ret = Typing::default();
            for (mxid, _) in room.typing.iter().filter(|(_, timeout)| **timeout > now) {
                ret.user_ids.insert(mxid.clone());
            }
            return Ok(Some(serde_json::to_value(ret).unwrap()));
        }
        Ok(room.ephemeral.get(event_type).cloned())
    }

    async fn set_ephemeral(
        &self,
        room_id: &RoomId,
        event_type: &str,
        content: Option<JsonValue>,
    ) -> Result<(), Error> {
        assert!(
            event_type != "m.typing",
            "m.typing should not be set directly"
        );
        let mut db = self.inner.write().await;
        let room = db.rooms.get_mut(room_id).ok_or(ErrorKind::RoomNotFound)?;
        match content {
            Some(c) => room.ephemeral.insert(String::from(event_type), c),
            None => room.ephemeral.remove(event_type),
        };
        let _ = room.notify_send.send(());
        Ok(())
    }

    async fn set_typing(
        &self,
        room_id: &RoomId,
        user_id: &MatrixId,
        is_typing: bool,
        timeout: u32,
    ) -> Result<(), Error> {
        let mut db = self.inner.write().await;
        let room = db.rooms.get_mut(room_id).ok_or(ErrorKind::RoomNotFound)?;
        if is_typing {
            room.typing.insert(
                user_id.clone(),
                Instant::now() + Duration::from_millis(timeout as u64),
            );
        } else {
            room.typing.remove(user_id);
        }
        let _ = room.notify_send.send(());

        Ok(())
    }

    async fn set_user_account_data(
        &self,
        username: &str,
        data: HashMap<String, JsonValue>,
    ) -> Result<(), Error> {
        let mut db = self.inner.write().await;
        if let Some(pos) = db.users.iter().position(|u| u.username == username) {
            db.users[pos].account_data = data;
            Ok(())
        } else {
            Err(ErrorKind::NotFound.into())
        }
    }
    async fn get_user_account_data(
        &self,
        username: &str,
    ) -> Result<HashMap<String, JsonValue>, Error> {
        let db = self.inner.read().await;
        let map = db
            .users
            .iter()
            .find(|u| u.username == username)
            .map(|u| u.account_data.clone())
            .unwrap_or(HashMap::new());
        Ok(map)
    }

    async fn get_batch(&self, id: &str) -> Result<Option<Batch>, Error> {
        let db = self.inner.read().await;
        Ok(db.batches.get(id).cloned())
    }

    async fn set_batch(&self, id: &str, batch: Batch) -> Result<(), Error> {
        let mut db = self.inner.write().await;
        let _ = db.batches.insert(String::from(id), batch);
        Ok(())
    }

    async fn print_the_world(&self) -> Result<(), Error> {
        let db = self.inner.read().await;
        println!("{:#?}", db.rooms);
        println!("{:#?}", db.users);
        println!("{:#?}", db.access_tokens);
        Ok(())
    }
}
