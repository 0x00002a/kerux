use actix_web::{
    get, post, put,
    web::{Data, Json, Path},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;
use tracing::{field::Empty, instrument, Span};

use crate::{
    client_api::auth::AccessToken,
    error::{Error, ErrorKind},
    events::presence::Status,
    storage::UserProfile,
    util::MatrixId,
    ServerState,
};

#[get("/profile/{user_id}/avatar_url")]
#[instrument(skip(state), err)]
pub async fn get_avatar_url(
    state: Data<Arc<ServerState>>,
    user_id: Path<MatrixId>,
) -> Result<Json<JsonValue>, Error> {
    if user_id.domain() != &state.config.domain {
        return Err(ErrorKind::Unimplemented.into());
    }

    let db = state.db_pool.get_handle().await?;
    let avatar_url = match db
        .get_profile(user_id.localpart())
        .await?
        .unwrap()
        .avatar_url
    {
        Some(v) => v,
        None => return Err(ErrorKind::NotFound.into()),
    };

    Ok(Json(json!({ "avatar_url": avatar_url })))
}

#[put("/profile/{user_id}/avatar_url")]
#[instrument(skip(state, token, body), fields(username = Empty), err)]
pub async fn set_avatar_url(
    state: Data<Arc<ServerState>>,
    token: AccessToken,
    req_id: Path<MatrixId>,
    body: Json<JsonValue>,
) -> Result<Json<()>, Error> {
    let db = state.db_pool.get_handle().await?;
    let username = db.try_auth(token.0).await?.ok_or(ErrorKind::UnknownToken)?;
    Span::current().record("username", username.as_str());

    if req_id.localpart() != username {
        return Err(ErrorKind::Forbidden.into());
    }
    if req_id.domain() != &state.config.domain {
        return Err(ErrorKind::Unknown("User does not live on this homeserver".to_string()).into());
    }

    let avatar_url = body
        .get("avatar_url")
        .ok_or(ErrorKind::BadJson(String::from("no avatar_url field")))?
        .as_str()
        .ok_or(ErrorKind::BadJson(String::from(
            "avatar_url should be a string",
        )))?;
    db.set_avatar_url(&username, avatar_url).await?;
    Ok(Json(()))
}

#[get("/profile/{user_id}/displayname")]
#[instrument(skip(state), err)]
pub async fn get_display_name(
    state: Data<Arc<ServerState>>,
    user_id: Path<MatrixId>,
) -> Result<Json<JsonValue>, Error> {
    if user_id.domain() != &state.config.domain {
        return Err(ErrorKind::Unknown("User does not live on this homeserver".to_string()).into());
    }

    let db = state.db_pool.get_handle().await?;
    let displayname = match db
        .get_profile(user_id.localpart())
        .await?
        .unwrap()
        .displayname
    {
        Some(v) => v,
        None => return Err(ErrorKind::NotFound.into()),
    };

    Ok(Json(json!({ "displayname": displayname })))
}

#[put("/profile/{user_id}/displayname")]
#[instrument(skip(state, token, body), fields(username = Empty), err)]
pub async fn set_display_name(
    state: Data<Arc<ServerState>>,
    token: AccessToken,
    req_id: Path<MatrixId>,
    body: Json<JsonValue>,
) -> Result<Json<()>, Error> {
    let db = state.db_pool.get_handle().await?;
    let username = db.try_auth(token.0).await?.ok_or(ErrorKind::UnknownToken)?;
    Span::current().record("username", username.as_str());

    if req_id.localpart() != username {
        return Err(ErrorKind::Forbidden.into());
    }
    if req_id.domain() != &state.config.domain {
        return Err(ErrorKind::Unknown("User does not live on this homeserver".to_string()).into());
    }

    let display_name = body
        .get("displayname")
        .ok_or(ErrorKind::BadJson(String::from("no displayname field")))?
        .as_str()
        .ok_or(ErrorKind::BadJson(String::from(
            "displayname should be a string",
        )))?;
    db.set_display_name(&username, display_name).await?;
    Ok(Json(()))
}

#[get("/profile/{user_id}")]
#[instrument(skip(state), err)]
pub async fn get_profile(
    state: Data<Arc<ServerState>>,
    user_id: Path<MatrixId>,
) -> Result<Json<JsonValue>, Error> {
    if user_id.domain() != &state.config.domain {
        return Err(ErrorKind::Unknown(format!(
            "User does not live on this homeserver (user domain: {} != server domain {})",
            user_id.domain(),
            state.config.domain
        ))
        .into());
    }

    let db = state.db_pool.get_handle().await?;
    let UserProfile {
        avatar_url,
        displayname,
        ..
    } = db.get_profile(user_id.localpart()).await?.unwrap();
    let mut response = serde_json::Map::new();
    if let Some(v) = avatar_url {
        response.insert("avatar_url".into(), v.into());
    }
    if let Some(v) = displayname {
        response.insert("displayname".into(), v.into());
    }

    Ok(Json(response.into()))
}

#[derive(Deserialize)]
pub struct UserDirSearchRequest {
    search_term: String,
    #[serde(default)]
    #[allow(unused)]
    limit: Option<usize>,
}

#[derive(Serialize)]
pub struct UserDirSearchResponse {
    results: Vec<User>,
    limited: bool,
}

#[derive(Serialize)]
struct User {
    user_id: MatrixId,
    #[serde(skip_serializing_if = "Option::is_none")]
    avatar_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    display_name: Option<String>,
}

//TODO: actually implement this
#[post("/user_directory/search")]
#[instrument(skip_all, err)]
pub async fn search_user_directory(
    state: Data<Arc<ServerState>>,
    req: Json<UserDirSearchRequest>,
) -> Result<Json<UserDirSearchResponse>, Error> {
    let req = req.into_inner();
    let db = state.db_pool.get_handle().await?;
    let searched_user = MatrixId::new(&req.search_term, state.config.domain.clone())
        .map_err(|e| ErrorKind::Unknown(e.to_string()))?;
    let user_profile = db.get_profile(searched_user.localpart()).await?;
    match user_profile {
        Some(p) => Ok(Json(UserDirSearchResponse {
            results: vec![User {
                user_id: searched_user,
                avatar_url: p.avatar_url,
                display_name: p.displayname,
            }],
            limited: false,
        })),
        None => Ok(Json(UserDirSearchResponse {
            results: Vec::new(),
            limited: false,
        })),
    }
}

#[derive(Serialize)]
pub struct Get3pidsResponse {
    threepids: Vec<Threepid>,
}

#[derive(Serialize)]
struct Threepid {
    medium: Medium,
    address: String,
    validated_at: u64,
    added_at: u64,
}

#[allow(dead_code)]
#[derive(Serialize)]
pub enum Medium {
    Email,
    // Phone number, including calling code
    Msisdn,
}

#[get("/account/3pid")]
#[instrument(skip_all, err)]
pub async fn get_3pids(
    _state: Data<Arc<ServerState>>,
    _token: AccessToken,
) -> Result<Json<Get3pidsResponse>, Error> {
    //TODO: implement
    Ok(Json(Get3pidsResponse {
        threepids: Vec::new(),
    }))
}

#[derive(Serialize, Debug)]
pub struct FilterEventsResponse {
    filter_id: String,
}

/// https://spec.matrix.org/v1.7/client-server-api/#post_matrixclientv3useruseridfilter
#[post("/user/{user_id}/filter")]
pub async fn filter_events() -> Result<Json<FilterEventsResponse>, Error> {
    // TODO: This should actually be implemented
    Ok(Json(FilterEventsResponse {
        filter_id: "todo".to_owned(),
    }))
}

#[get("/user/{user_id}/filter/{filter_id}")]
pub async fn filter_event() -> Result<Json<serde_json::Value>, Error> {
    // TODO: This should actually be implemented
    Ok(Json(json!({})))
}
#[derive(Deserialize, Debug)]
#[repr(transparent)]
#[serde(transparent)]
pub struct StatusRequest(Status);

#[put("/presence/{user_id}/status")]
pub async fn status(
    state: Data<Arc<ServerState>>,
    user_id: Path<MatrixId>,
    req: Json<StatusRequest>,
) -> Result<Json<serde_json::Value>, Error> {
    let user_id = user_id.into_inner();
    state
        .db_pool
        .get_handle()
        .await?
        .set_status(user_id.localpart(), req.0 .0)
        .await?;
    Ok(Json(json!({})))
}

#[get("/user/{user_id}/account_data/{type}")]
pub async fn account_data(
    state: Data<Arc<ServerState>>,
    path: Path<(MatrixId, String)>,
    token: AccessToken,
) -> Result<Json<serde_json::Value>, Error> {
    let (_, data_type) = path.into_inner();
    let db = state.db_pool.get_handle().await?;
    let username = db.try_auth(token.0).await?.ok_or(ErrorKind::UnknownToken)?;
    Span::current().record("username", username.as_str());
    let data = db.get_user_account_data(&username).await?;
    let result = data.get(&data_type).ok_or(ErrorKind::NotFound)?;
    Ok(Json(result.to_owned()))
}

#[put("/user/{user_id}/account_data/{type}")]
pub async fn account_data_update(
    state: Data<Arc<ServerState>>,
    path: Path<(MatrixId, String)>,
    token: AccessToken,
    value: Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, Error> {
    let (_, data_type) = path.into_inner();
    let db = state.db_pool.get_handle().await?;
    let username = db.try_auth(token.0).await?.ok_or(ErrorKind::UnknownToken)?;
    Span::current().record("username", username.as_str());
    db.set_user_account_data_value(&username, data_type, value.0)
        .await?;
    Ok(Json(json!({})))
}
