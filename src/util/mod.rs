use actix_web::{
    body::EitherBody,
    http::{self, StatusCode},
    post,
    web::Data,
    HttpResponse, Responder,
};
use serde::Serialize;
use std::sync::Arc;

use crate::ServerState;

pub mod domain;
pub mod mxid;
pub mod storage;

pub use mxid::{MatrixId, MxidError};
pub use storage::StorageExt;

#[post("/_debug/print_the_world")]
pub async fn print_the_world(state: Data<Arc<ServerState>>) -> String {
    let db = state.db_pool.get_handle().await.unwrap();
    db.print_the_world().await.unwrap();
    String::new()
}

#[derive(Debug, PartialEq, Eq)]
pub struct JsonWithCode<T> {
    value: T,
    code: http::StatusCode,
}

impl<T> JsonWithCode<T> {
    pub fn new(value: T, code: http::StatusCode) -> Self {
        Self { value, code }
    }
    pub fn ok(value: T) -> Self {
        Self::new(value, StatusCode::OK)
    }
}

impl<T: Serialize> Responder for JsonWithCode<T> {
    type Body = EitherBody<String>;

    fn respond_to(self, _: &actix_web::HttpRequest) -> actix_web::HttpResponse<Self::Body> {
        match serde_json::to_string(&self.value) {
            Ok(v) => match HttpResponse::Ok().status(self.code).message_body(v) {
                Ok(r) => r.map_into_left_body(),
                Err(e) => HttpResponse::from_error(e).map_into_right_body(),
            },
            Err(e) => HttpResponse::from_error(e).map_into_right_body(),
        }
    }
}
