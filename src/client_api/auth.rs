use actix_web::{
    dev::Payload,
    get,
    http::StatusCode,
    post,
    web::{self, Data, Json},
    FromRequest, HttpRequest,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{convert::TryFrom, sync::Arc};
use tracing::{field::Empty, instrument, span::Span};
use uuid::Uuid;

use crate::{
    error::{Error, ErrorKind},
    util::{JsonWithCode, MatrixId},
    ServerState,
};

#[derive(Debug, Deserialize, Serialize)]
enum LoginType {
    #[serde(rename = "m.login.password")]
    Password,
}

#[derive(Debug)]
pub struct AccessToken(pub Uuid);

impl FromRequest for AccessToken {
    type Error = Error;
    type Future = futures::future::Ready<Result<Self, Self::Error>>;
    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        let res = (|| {
            if let Some(s) = req.headers().get("Authorization") {
                let s: &str = s.to_str().map_err(|_| ErrorKind::MissingToken)?;
                if !s.starts_with("Bearer ") {
                    return Err(ErrorKind::MissingToken);
                }
                let token = s
                    .trim_start_matches("Bearer ")
                    .parse()
                    .map_err(|_| ErrorKind::UnknownToken)?;
                Ok(token)
            } else if let Some(pair) = req
                .uri()
                .query()
                .ok_or(ErrorKind::MissingToken)?
                .split('&')
                .find(|pair| pair.starts_with("access_token"))
            {
                let token = pair
                    .trim_start_matches("access_token=")
                    .parse()
                    .map_err(|_| ErrorKind::UnknownToken)?;
                Ok(token)
            } else {
                Err(ErrorKind::MissingToken)
            }
        })();
        match res {
            Ok(token) => futures::future::ok(AccessToken(token)),
            Err(e) => futures::future::err(e.into()),
        }
    }
}

#[get("/login")]
#[instrument]
pub async fn get_supported_login_types() -> Json<serde_json::Value> {
    //TODO: allow config
    Json(json!({
        "flows": [
            {
                "type": "m.login.password"
            }
        ]
    }))
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    #[serde(rename = "type")]
    login_type: LoginType,
    identifier: Identifier,
    password: Option<String>,
    token: Option<String>,
    device_id: Option<String>,
    initial_device_display_name: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum Identifier {
    #[serde(rename = "m.id.user")]
    Username { user: String },
    #[serde(rename = "m.id.thirdparty")]
    ThirdParty { medium: String, address: String },
    #[serde(rename = "m.id.phone")]
    Phone { country: String, phone: String },
}

#[derive(Serialize)]
pub struct LoginResponse {
    user_id: MatrixId,
    access_token: String,
    device_id: String,
    //TODO: This is deprecated, but Fractal is the only client that doesn't require it. Remove it
    // once all the other clients have updated to current spec
    home_server: String,
}

#[post("/login")]
#[instrument(skip_all, err)]
pub async fn login(
    state: Data<Arc<ServerState>>,
    req: Json<LoginRequest>,
) -> Result<Json<LoginResponse>, Error> {
    let req = req.into_inner();

    let username = match req.identifier {
        Identifier::Username { user } => {
            let res = MatrixId::try_from(&*user);
            match res {
                Ok(mxid) => mxid.localpart().to_string(),
                Err(_) => user,
            }
        }
        _ => return Err(ErrorKind::Unimplemented.into()),
    };
    let password = req.password.ok_or(ErrorKind::Unimplemented)?;

    let db = state.db_pool.get_handle().await?;
    if !db.verify_password(&username, &password).await? {
        return Err(ErrorKind::Forbidden.into());
    }

    let device_id = req
        .device_id
        .unwrap_or(format!("{:08X}", rand::random::<u32>()));
    let access_token = db.create_access_token(&username, &device_id).await?;

    tracing::info!(username = username.as_str(), "User logged in");

    let user_id = MatrixId::new(&username, state.config.domain.clone()).unwrap();
    let access_token = format!("{}", access_token.hyphenated());

    Ok(Json(LoginResponse {
        user_id,
        access_token,
        device_id,
        home_server: state.config.domain.to_string(),
    }))
}

#[post("/logout")]
#[instrument(skip(state), err)]
pub async fn logout(state: Data<Arc<ServerState>>, token: AccessToken) -> Result<Json<()>, Error> {
    let db = state.db_pool.get_handle().await?;
    db.delete_access_token(token.0).await?;
    Ok(Json(()))
}

#[post("/logout/all")]
#[instrument(skip(state), err)]
pub async fn logout_all(
    state: Data<Arc<ServerState>>,
    token: AccessToken,
) -> Result<Json<()>, Error> {
    let db = state.db_pool.get_handle().await?;
    db.delete_all_access_tokens(token.0).await?;
    Ok(Json(()))
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    auth: Option<serde_json::Value>,
    username: Option<String>,
    password: Option<String>,
    device_id: Option<String>,
    initial_device_display_name: Option<String>,
    #[serde(default)]
    inhibit_login: bool,
}
#[derive(Debug, Serialize)]
struct RegisterSupportedResponse {
    flows: Vec<LoginType>,
    params: serde_json::Value,
    session: String,
}

#[derive(Debug, Serialize)]
pub struct CheckUsernameAvailableResponse {
    available: bool,
}

#[derive(Debug, Deserialize)]
pub struct CheckUsernameAvailableParams {
    username: String,
}

#[get("/register/available")]
pub async fn check_username_available(
    state: Data<Arc<ServerState>>,
    query: web::Query<CheckUsernameAvailableParams>,
) -> Result<Json<CheckUsernameAvailableResponse>, Error> {
    let username = query.0.username;
    let db = state.db_pool.get_handle().await?;
    let exists = db.get_user_account_data(&username).await.is_ok();
    Ok(Json(CheckUsernameAvailableResponse { available: !exists }))
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum UserType {
    Guest,
    User,
}
impl Default for UserType {
    fn default() -> Self {
        Self::User
    }
}

#[derive(Debug, Deserialize)]
pub struct RegisterParams {
    #[serde(default)]
    kind: UserType,
}

#[post("/register")]
#[instrument(skip_all, fields(username = Empty), err)]
pub async fn register(
    state: Data<Arc<ServerState>>,
    req: Json<RegisterRequest>,
    params: web::Query<RegisterParams>,
) -> Result<JsonWithCode<serde_json::Value>, Error> {
    let req = req.into_inner();
    if req.password.is_none() && req.auth.is_none() {
        return Ok(JsonWithCode::new(
            serde_json::to_value(RegisterSupportedResponse {
                flows: vec![LoginType::Password],
                params: json!({}),
                session: "".to_string(),
            })
            .unwrap(),
            StatusCode::UNAUTHORIZED,
        ));
    }
    if let UserType::Guest = params.0.kind {
        return Err(ErrorKind::Unimplemented.into());
    }

    Span::current().record("username", req.username.as_deref());

    let user_id = req
        .username
        .map(|u| MatrixId::new(&u, state.config.domain.clone()))
        .unwrap_or_else(|| MatrixId::new_with_random_local(state.config.domain.clone()))
        .map_err(|e| ErrorKind::BadJson(format!("{}", e)))?;

    let db = state.db_pool.get_handle().await?;
    db.create_user(
        user_id.localpart(),
        &req.password
            .ok_or_else(|| Error::from(ErrorKind::BadJson("missing password".to_owned())))?,
    )
    .await?;
    if req.inhibit_login {
        return Ok(JsonWithCode::ok(json!({
            "user_id": user_id.localpart()
        })));
    }

    let device_id = req
        .device_id
        .unwrap_or(format!("{:08X}", rand::random::<u32>()));
    let access_token = db
        .create_access_token(user_id.localpart(), &device_id)
        .await?;
    let access_token = format!("{}", access_token.hyphenated());

    Ok(JsonWithCode::ok(json!({
        "user_id": user_id,
        "access_token": access_token,
        "device_id": device_id
    })))
}
