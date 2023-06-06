use actix_web::{get, web::Json};
use serde::Serialize;

#[derive(Serialize, Debug)]
pub struct PushCondition {
    is: String,
    key: String,
    kind: String,
    pattern: String,
}

#[derive(Serialize, Debug)]
pub struct PushRule {
    actions: Vec<serde_json::Value>,
    conditions: Option<Vec<PushCondition>>,
    default: bool,
    enabled: bool,
    pattern: Option<String>,
    rule_id: Option<String>,
}
#[derive(Serialize, Debug, Default)]
pub struct Ruleset {
    content: Vec<PushRule>,
    #[serde(rename = "override")]
    override_: Vec<PushRule>,
    room: Vec<PushRule>,
    sender: Vec<PushRule>,
    underride: Vec<PushRule>,
}

#[derive(Serialize, Debug)]
pub struct GlobalPushRules {
    global: Ruleset,
}

/// https://spec.matrix.org/v1.7/client-server-api/#get_matrixclientv3pushrules
#[get("/pushrules/")]
pub async fn global() -> Json<GlobalPushRules> {
    Json(GlobalPushRules {
        global: Default::default(),
    })
}
