use actix_web::{
    put,
    web::{Data, Json, Path},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{field::Empty, instrument, Span};

use crate::{
    client_api::auth::AccessToken,
    error::{Error, ErrorKind},
    util::{mxid::RoomId, MatrixId},
    ServerState,
};

#[derive(Deserialize)]
pub struct TypingRequest {
    typing: bool,
    #[serde(default)]
    timeout: u32,
}

#[put("/rooms/{room_id}/typing/{user_id}")]
#[instrument(skip(state, token, req), fields(username = Empty), err)]
pub async fn typing(
    state: Data<Arc<ServerState>>,
    token: AccessToken,
    path: Path<(RoomId, MatrixId)>,
    req: Json<TypingRequest>,
) -> Result<Json<Value>, Error> {
    let (room_id, user_id) = path.into_inner();
    let db = state.db_pool.get_handle().await?;
    let username = db.try_auth(token.0).await?.ok_or(ErrorKind::Forbidden)?;
    Span::current().record("username", username.as_str());

    if (username.as_str(), &state.config.domain) != (user_id.localpart(), user_id.domain()) {
        return Err(ErrorKind::Forbidden.into());
    }
    db.set_typing(&room_id, &user_id, req.typing, req.timeout)
        .await?;
    Ok(Json(json!({})))
}
