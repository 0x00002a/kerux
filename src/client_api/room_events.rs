use actix_web::{
    get, put,
    web::{Data, Json, Path, Query},
};
use futures::FutureExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use std::{collections::HashMap, sync::Arc};
use tokio::time::{sleep, Duration};
use tracing::{field::Empty, instrument, Span};

use crate::{
    client_api::auth::AccessToken,
    error::{Error, ErrorKind},
    events::{room::Membership, Event, EventContent},
    storage::{EventQuery, QueryType},
    util::{storage::NewEvent, MatrixId, StorageExt},
    ServerState,
};

/// Provided in URL query params
#[derive(Debug, Deserialize)]
pub struct SyncRequest {
    #[serde(default)]
    filter: Option<String>,
    #[serde(default)]
    since: Option<String>,
    #[serde(default)]
    full_state: bool,
    #[serde(default = "default_set_presence")]
    set_presence: SetPresence,
    #[serde(default)]
    timeout: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename = "snake_case")]
enum SetPresence {
    Offline,
    Online,
    Unavailable,
}

fn default_set_presence() -> SetPresence {
    SetPresence::Online
}

#[derive(Debug, Serialize)]
pub struct SyncResponse {
    next_batch: String,
    rooms: Rooms,
    presence: Presence,
    account_data: AccountData,
}

#[derive(Debug, Default, Serialize)]
struct Rooms {
    join: HashMap<String, JoinedRoom>,
    invite: HashMap<String, InvitedRoom>,
    leave: HashMap<String, LeftRoom>,
}

#[derive(Debug, Serialize)]
struct JoinedRoom {
    summary: RoomSummary,
    state: State,
    timeline: Timeline,
    ephemeral: Ephemeral,
    account_data: AccountData,
}

#[derive(Debug, Serialize)]
struct RoomSummary {
    #[serde(rename = "m.heroes")]
    heroes: Option<Vec<String>>,
    #[serde(rename = "m.joined_member_count")]
    joined_member_count: usize,
    #[serde(rename = "m.invited_member_count")]
    invited_member_count: usize,
}

#[derive(Debug, Serialize)]
struct State {
    events: Vec<Event>,
}

#[derive(Debug, Serialize)]
struct Timeline {
    events: Vec<Event>,
    limited: bool,
    prev_batch: String,
}

#[derive(Debug, Serialize)]
struct Ephemeral {
    events: Vec<KvPair>,
}

/// This is referred to as `Event` in the Matrix spec, but we already have a thing called event
/// and it doesn't really make sense to call it that.
#[derive(Debug, Serialize)]
struct KvPair {
    content: JsonValue,
    #[serde(rename = "type")]
    ty: String,
}

#[derive(Debug, Serialize)]
struct AccountData {
    events: Vec<KvPair>,
}

#[derive(Debug, Serialize)]
struct InvitedRoom {
    invite_state: InviteState,
}

#[derive(Debug, Serialize)]
struct InviteState {
    events: Vec<StrippedState>,
}

#[derive(Debug, Serialize)]
struct StrippedState {
    #[serde(flatten)]
    content: EventContent,
    state_key: String,
    sender: MatrixId,
}

#[derive(Debug, Serialize)]
struct LeftRoom {
    state: State,
    timeline: Timeline,
    account_data: AccountData,
}

#[derive(Debug, Serialize)]
struct Presence {
    events: Vec<KvPair>,
}
#[derive(Deserialize, Debug, PartialEq, Eq, Copy, Clone, Hash)]
enum MessageOrdering {
    #[serde(rename = "f")]
    Forward,
    #[serde(rename = "b")]
    Backward,
}
impl Default for MessageOrdering {
    fn default() -> Self {
        Self::Forward
    }
}

#[derive(Deserialize, Debug)]
pub struct MessagesParams {
    from: Option<String>,
    filter: Option<serde_json::Value>,
    limit: Option<i32>,
    to: Option<String>,
    #[serde(default)]
    dir: MessageOrdering,
}
#[derive(Serialize, Debug)]
pub struct MessagesResponse {
    chunk: Vec<Event>,
    start: String,
}

#[get("/rooms/{room_id}/messages")]
#[instrument(skip(state, token), fields(username = Empty), err)]
pub async fn messages(
    state: Data<Arc<ServerState>>,
    token: AccessToken,
    room_id: Path<String>,
    params: Query<MessagesParams>,
) -> Result<Json<MessagesResponse>, Error> {
    let db = state.db_pool.get_handle().await?;
    let username = db.try_auth(token.0).await?.ok_or(ErrorKind::UnknownToken)?;
    Span::current().record("username", username.as_str());
    let filter = params.filter.as_ref().filter(|f| f.is_object());
    let query = EventQuery {
        room_id: room_id.as_str(),
        query_type: QueryType::Timeline {
            from: params
                .from
                .as_ref()
                .and_then(|p| p.parse().ok())
                .unwrap_or_default(),
            to: None,
        },
        contains_json: filter.cloned(),
        senders: &[],
        not_senders: &[],
        types: &[],
        not_types: &[],
    };
    let (mut events, _) = db.query_events(query, false).await?;
    let start = events
        .first()
        .map(|e| e.state_key.as_deref().unwrap_or("empty"))
        .unwrap_or("empty")
        .to_owned();
    if params.dir == MessageOrdering::Backward {
        events.reverse();
    }
    Ok(Json(MessagesResponse {
        chunk: events,
        start,
    }))
}

#[get("/sync")]
#[instrument(skip_all, fields(username = Empty), err)]
pub async fn sync(
    state: Data<Arc<ServerState>>,
    token: AccessToken,
    req: Query<SyncRequest>,
) -> Result<Json<SyncResponse>, Error> {
    let db = state.db_pool.get_handle().await?;
    let username = db.try_auth(token.0).await?.ok_or(ErrorKind::UnknownToken)?;
    Span::current().record("username", &username.as_str());
    let user_id = MatrixId::new(&username, state.config.domain.clone()).unwrap();

    let mut batch = db
        .get_batch(req.since.as_deref().unwrap_or("empty"))
        .await?
        .unwrap_or_default();
    let next_batch_id = format!("{:x}", rand::random::<u64>());
    let mut res = SyncResponse {
        next_batch: next_batch_id.clone(),
        rooms: Rooms::default(),
        presence: Presence { events: Vec::new() },
        account_data: AccountData { events: Vec::new() },
    };

    let rooms = db.get_rooms().await?;
    let mut memberships = HashMap::new();
    for room_id in rooms.iter() {
        if let Some(membership) = db.get_membership(&user_id, room_id).await? {
            memberships.insert(room_id, membership);
        }
    }
    let mut something_happened = false;
    for (&room_id, membership) in memberships.iter() {
        match membership {
            Membership::Join => {
                batch.invites.remove(room_id);
                let from = batch.rooms.get(room_id).map(|v| *v).unwrap_or(0);
                let (events, progress) = db
                    .query_events(
                        EventQuery {
                            query_type: QueryType::Timeline { from, to: None },
                            room_id,
                            senders: &[],
                            not_senders: &[],
                            types: &[],
                            not_types: &[],
                            contains_json: None,
                        },
                        false,
                    )
                    .await?;
                batch.rooms.insert(room_id.clone(), progress + 1);

                let mut state_events = Vec::new();
                if req.full_state {
                    state_events = db.get_full_state(&room_id).await?;
                }

                if !events.is_empty() || !state_events.is_empty() {
                    something_happened = true;
                }
                let (joined, invited) = db.get_room_member_counts(&room_id).await?;
                let summary = RoomSummary {
                    heroes: None,
                    joined_member_count: joined,
                    invited_member_count: invited,
                };
                let state = State {
                    events: state_events,
                };
                let timeline = Timeline {
                    events,
                    limited: false,
                    prev_batch: String::from("empty"),
                };
                let ephemeral = Ephemeral {
                    events: db
                        .get_all_ephemeral(room_id)
                        .await?
                        .into_iter()
                        .map(|(k, v)| KvPair { ty: k, content: v })
                        .collect(),
                };
                let account_data = AccountData { events: Vec::new() };
                res.rooms.join.insert(
                    String::from(room_id),
                    JoinedRoom {
                        summary,
                        state,
                        timeline,
                        ephemeral,
                        account_data,
                    },
                );
            }
            Membership::Invite if !batch.invites.contains(room_id) => {
                let events = db
                    .get_full_state(&room_id)
                    .await?
                    .into_iter()
                    .map(|e| StrippedState {
                        content: e.event_content,
                        state_key: e.state_key.unwrap(),
                        sender: e.sender,
                    })
                    .collect();
                res.rooms.invite.insert(
                    room_id.clone(),
                    InvitedRoom {
                        invite_state: InviteState { events },
                    },
                );
                batch.invites.insert(room_id.clone());
            }
            _ => {}
        }
    }

    if something_happened {
        db.set_batch(&next_batch_id, batch).await?;
        return Ok(Json(res));
    }

    let mut queries = Vec::new();
    for (&room_id, _) in memberships.iter().filter(|(_, m)| **m == Membership::Join) {
        let from = batch.rooms.get(room_id).map(|v| *v).unwrap_or(0);
        let room_id_clone = String::from(room_id);
        queries.push(
            db.query_events(
                EventQuery {
                    query_type: QueryType::Timeline { from, to: None },
                    room_id,
                    senders: &[],
                    not_senders: &[],
                    types: &[],
                    not_types: &[],
                    contains_json: None,
                },
                true,
            )
            .map(move |r| (r, room_id_clone)),
        );
    }
    if queries.is_empty() {
        // user is not in any rooms. when we have a better event system we can wait for
        // invitations etc, but for now just sleep so the client doesn't sync over and over
        db.set_batch(&next_batch_id, batch).await?;
        sleep(Duration::from_millis(req.timeout as _)).await;
        return Ok(Json(res));
    }

    let timeout = sleep(Duration::from_millis(req.timeout as _));
    tokio::select! {
        _ = timeout => {
            db.set_batch(&next_batch_id, batch).await?;
            return Ok(Json(res));
        },
        ((query_res, room_id), _, _) = futures::future::select_all(queries) => {
            let (events, progress) = query_res?;
            let (joined, invited) = db.get_room_member_counts(&room_id).await?;
            let summary = RoomSummary {
                heroes: None,
                joined_member_count: joined,
                invited_member_count: invited,
            };
            batch.rooms.insert(room_id.clone(), progress + 1);
            res.rooms.join.insert(
                room_id.clone(),
                JoinedRoom {
                    summary,
                    timeline: Timeline {
                        events,
                        limited: false,
                        prev_batch: String::from("empty"),
                    },
                    state: State { events: Vec::new() },
                    ephemeral: Ephemeral {
                        events: db.get_all_ephemeral(&room_id).await?.into_iter().map(
                            |(k, v)| KvPair {
                                ty: k,
                                content: v,
                            }).collect()
                    },
                    account_data: AccountData { events: Vec::new() },
                }
            );
            db.set_batch(&next_batch_id, batch).await?;
            return Ok(Json(res));
        },
    };
}

#[get("/rooms/{room_id}/event/{event_id}")]
#[instrument(skip(state, token), fields(username = Empty), err)]
pub async fn get_event(
    state: Data<Arc<ServerState>>,
    token: AccessToken,
    path: Path<(String, String)>,
) -> Result<Json<Event>, Error> {
    let (room_id, event_id) = path.into_inner();
    let db = state.db_pool.get_handle().await?;

    let username = db.try_auth(token.0).await?.ok_or(ErrorKind::UnknownToken)?;
    Span::current().record("username", &username.as_str());
    let user_id = MatrixId::new(&username, state.config.domain.clone()).unwrap();

    if db.get_membership(&user_id, &room_id).await? != Some(Membership::Join) {
        return Err(ErrorKind::Forbidden.into());
    }

    match db.get_pdu(&room_id, &event_id).await? {
        Some(pdu) => Ok(Json(pdu.into_client_format())),
        None => Err(ErrorKind::NotFound.into()),
    }
}

#[get("/rooms/{room_id}/state/{event_id}")]
pub async fn get_state_event_no_key(
    state: Data<Arc<ServerState>>,
    token: AccessToken,
    path_args: Path<(String, String)>,
) -> Result<Json<Event>, Error> {
    let (room_id, event_type) = path_args.into_inner();
    get_state_event_inner(state, token, (room_id, event_type, String::new())).await
}

#[get("/rooms/{room_id}/state/{event_id}/{state_key}")]
pub async fn get_state_event_key(
    state: Data<Arc<ServerState>>,
    token: AccessToken,
    path_args: Path<(String, String, String)>,
) -> Result<Json<Event>, Error> {
    get_state_event_inner(state, token, path_args.into_inner()).await
}

#[instrument(skip(state, token), fields(username = Empty), err)]
pub async fn get_state_event_inner(
    state: Data<Arc<ServerState>>,
    token: AccessToken,
    (room_id, event_type, state_key): (String, String, String),
) -> Result<Json<Event>, Error> {
    let db = state.db_pool.get_handle().await?;
    let username = db.try_auth(token.0).await?.ok_or(ErrorKind::UnknownToken)?;
    Span::current().record("username", &username.as_str());
    let user_id = MatrixId::new(&username, state.config.domain.clone()).unwrap();

    if db.get_membership(&user_id, &room_id).await? != Some(Membership::Join) {
        return Err(ErrorKind::Forbidden.into());
    }

    match db
        .get_state_event(&room_id, &event_type, &state_key)
        .await?
    {
        Some(event) => Ok(Json(event)),
        None => Err(ErrorKind::NotFound.into()),
    }
}

#[get("/rooms/{room_id}/state")]
#[instrument(skip(state, token), fields(username = Empty), err)]
pub async fn get_state(
    state: Data<Arc<ServerState>>,
    token: AccessToken,
    room_id: Path<String>,
) -> Result<Json<Vec<Event>>, Error> {
    let db = state.db_pool.get_handle().await?;
    let username = db.try_auth(token.0).await?.ok_or(ErrorKind::UnknownToken)?;
    Span::current().record("username", &username.as_str());
    let user_id = MatrixId::new(&username, state.config.domain.clone()).unwrap();

    match db.get_membership(&user_id, &room_id).await? {
        Some(Membership::Join) => {}
        Some(_) => return Err(ErrorKind::Unimplemented.into()),
        None => return Err(ErrorKind::Forbidden.into()),
    }

    let state = db.get_full_state(&room_id).await?;
    Ok(Json(state))
}

#[derive(Deserialize)]
pub struct MembersRequest {
    at: String,
    #[serde(default)]
    membership: Option<Membership>,
    #[serde(default)]
    not_membership: Option<Membership>,
}

#[derive(Serialize)]
pub struct MembersResponse {
    chunk: Vec<Event>,
}

#[get("/rooms/{room_id}/members")]
#[instrument(skip(state, token, req), fields(username = Empty), err)]
pub async fn get_members(
    state: Data<Arc<ServerState>>,
    token: AccessToken,
    room_id: Path<String>,
    req: Query<MembersRequest>,
) -> Result<Json<MembersResponse>, Error> {
    let db = state.db_pool.get_handle().await?;
    let username = db.try_auth(token.0).await?.ok_or(ErrorKind::UnknownToken)?;
    Span::current().record("username", &username.as_str());
    let user_id = MatrixId::new(&username, state.config.domain.clone()).unwrap();

    match db.get_membership(&user_id, &room_id).await? {
        Some(Membership::Join) => {}
        Some(_) => return Err(ErrorKind::Unimplemented.into()),
        None => return Err(ErrorKind::Forbidden.into()),
    }

    let mut state = db.get_full_state(&room_id).await?;
    state.retain(|event| {
        if let EventContent::Member(ref content) = &event.event_content {
            let membership = &content.membership;
            (if let Some(filter) = &req.membership {
                membership == filter
            } else {
                true
            } && if let Some(exclude) = &req.not_membership {
                membership != exclude
            } else {
                true
            })
        } else {
            false
        }
    });

    Ok(Json(MembersResponse { chunk: state }))
}

#[derive(Serialize)]
pub struct SendEventResponse {
    event_id: String,
}

#[put("/rooms/{room_id}/state/{event_type}/{state_key}")]
#[instrument(skip(state, token, event_content), fields(username = Empty), err)]
pub async fn send_state_event(
    state: Data<Arc<ServerState>>,
    token: AccessToken,
    path: Path<(String, String, String)>,
    event_content: Json<JsonValue>,
) -> Result<Json<SendEventResponse>, Error> {
    let (room_id, event_type, state_key) = path.into_inner();
    let db = state.db_pool.get_handle().await?;
    let username = db.try_auth(token.0).await?.ok_or(ErrorKind::UnknownToken)?;
    Span::current().record("username", &username.as_str());
    let user_id = MatrixId::new(&username, state.config.domain.clone()).unwrap();

    let event = NewEvent {
        event_content: EventContent::new(&event_type, event_content.into_inner())?,
        sender: user_id,
        state_key: Some(state_key),
        redacts: None,
        unsigned: None,
    };

    let event_id = db.add_event(&room_id, event, &state.state_resolver).await?;

    tracing::trace!(event_id = &event_id.as_str(), "Added event");

    Ok(Json(SendEventResponse { event_id }))
}

#[put("/rooms/{room_id}/send/{event_type}/{txn_id}")]
#[instrument(skip(state, token, event_content), fields(username = Empty), err)]
pub async fn send_event(
    state: Data<Arc<ServerState>>,
    token: AccessToken,
    path: Path<(String, String, String)>,
    event_content: Json<JsonValue>,
) -> Result<Json<SendEventResponse>, Error> {
    let (room_id, event_type, txn_id) = path.into_inner();
    let db = state.db_pool.get_handle().await?;
    let username = db.try_auth(token.0).await?.ok_or(ErrorKind::UnknownToken)?;
    Span::current().record("username", &username.as_str());
    if !db.record_txn(token.0, txn_id.clone()).await? {
        return Err(ErrorKind::TxnIdExists.into());
    }
    let user_id = MatrixId::new(&username, state.config.domain.clone()).unwrap();

    let event = NewEvent {
        event_content: EventContent::new(&event_type, event_content.into_inner())?,
        sender: user_id.clone(),
        state_key: None,
        redacts: None,
        unsigned: Some(json!({ "transaction_id": txn_id })),
    };

    //TODO: is this right in the eyes of the spec? also does it matter?
    db.set_typing(&room_id, &user_id, false, 0).await?;
    let event_id = db.add_event(&room_id, event, &state.state_resolver).await?;

    tracing::trace!(event_id = &event_id.as_str(), "Added event");

    Ok(Json(SendEventResponse { event_id }))
}
