use std::{collections::BTreeMap, sync::Arc};

use actix_web::{
    get, post,
    web::{Data, Json},
};
use serde::{Deserialize, Serialize};
use tracing::Span;

use crate::{
    error::{Error, ErrorKind},
    util::MatrixId,
    ServerState,
};

use super::auth::AccessToken;

#[derive(Debug, Deserialize)]
#[repr(transparent)]
#[serde(transparent)]
struct Timeout(u128);
impl Default for Timeout {
    fn default() -> Self {
        Self(std::time::Duration::from_secs(10).as_millis())
    }
}

#[allow(dead_code)] // TODO: implement e2e
#[derive(Debug, Deserialize)]
pub struct QueryRequest {
    device_keys: BTreeMap<MatrixId, Vec<String>>,
    #[serde(default)]
    timeout: Timeout,
}
#[derive(Debug, Serialize)]
pub struct QueryResponse {}

#[post("/keys/query")]
pub async fn query(
    state: Data<Arc<ServerState>>,
    _req: Json<QueryRequest>,
    token: AccessToken,
) -> Result<Json<QueryResponse>, Error> {
    let db = state.db_pool.get_handle().await?;
    let username = db.try_auth(token.0).await?.ok_or(ErrorKind::UnknownToken)?;
    Span::current().record("username", username.as_str());
    Ok(Json(QueryResponse {}))
}
