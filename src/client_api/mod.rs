use actix_web::{
    get,
    web::{self, Json},
    Scope,
};
use serde_json::json;

mod auth;
mod ephemeral;
mod keys;
mod pushrules;
mod room;
mod room_events;
mod user;

pub fn configure_endpoints(cfg: &mut web::ServiceConfig) {
    cfg.service(versions);
    let mount = |scope: Scope| {
        scope
            .service(auth::get_supported_login_types)
            .service(auth::login)
            .service(auth::logout)
            .service(auth::logout_all)
            .service(auth::register)
            .service(auth::check_username_available)
            .service(user::get_avatar_url)
            .service(user::set_avatar_url)
            .service(user::get_display_name)
            .service(user::set_display_name)
            .service(user::get_profile)
            .service(user::search_user_directory)
            .service(user::get_3pids)
            .service(user::filter_events)
            .service(user::filter_event)
            .service(user::status)
            .service(user::account_data)
            .service(user::account_data_update)
            .service(room::create_room)
            .service(room::invite)
            .service(room::join_by_id_or_alias)
            .service(room_events::sync)
            .service(room_events::get_event)
            .service(room_events::get_state_event_no_key)
            .service(room_events::get_state_event_key)
            .service(room_events::get_state)
            .service(room_events::get_members)
            .service(room_events::send_state_event)
            .service(room_events::send_event)
            .service(room_events::messages)
            .service(ephemeral::typing)
            .service(thirdparty_protocols)
            .service(keys::query)
    };

    cfg.service(mount(web::scope("/r0")));
    cfg.service(mount(web::scope("/v3")));
}

#[get("/versions")]
async fn versions() -> Json<serde_json::Value> {
    Json(json!({
        "versions": [
            "v1.7",
        ]
    }))
}

#[get("/thirdparty/protocols")]
async fn thirdparty_protocols() -> Json<serde_json::Value> {
    Json(json!({}))
}
