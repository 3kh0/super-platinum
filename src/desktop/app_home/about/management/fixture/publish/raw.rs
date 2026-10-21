//! Original closed-server values; local proposed metadata is never applied here.
use super::*;
pub(super) fn seed(workflow: &mut Value, trigger: &mut Value) {
    workflow["id"] = json!("WfREVIEW01");
    workflow["is_published"] = json!(true);
    workflow["last_published_version_id"] = json!("V1");
    workflow["unpublished_change_count"] = json!(2);
    trigger["workflow"]["workflow_id"] = json!("WfREVIEW01");
    trigger["trigger_type_id"] = json!("TT_SHORTCUT");
    workflow["steps"][0]["function"]["id"] = json!("FnSTATIC");
    workflow["steps"][0]["function"]["callback_id"] = json!("noop");
    workflow["steps"][0]["function"]["type"] = json!("builtin");
    workflow["steps"][0]["function"]["input_parameters"] = json!([]);
    workflow["steps"][0]["errors"] = json!([]);
    workflow["input_parameters"] = json!({"keep":{},"removed":{},"custom":{}});
    trigger["inputs"] = json!({"keep":{"value":"literal source","opaque":{"preserve":[1,true,"original"]}},"removed":{"value":"remove"},"custom":false});
    trigger["available_data"] = json!({
        "{{data.custom}}":{"name":"custom","type":"string","title":"Custom","is_customizable":true,"description":"Original custom input"},
        "{{data.channel}}":{"name":"channel","type":"slack#/types/channel_id","is_customizable":false,"title":"Channel"}
    });
}
pub(super) fn published(original: &Value) -> Value {
    let mut value = original.clone();
    value["is_published"] = json!(true);
    value["last_published_version_id"] = json!("V2");
    value["unpublished_change_count"] = json!(0);
    value["date_updated"] = json!(2);
    value["input_parameters"] = json!({"keep":{},"custom":{},"channel":{}});
    value
}
pub(super) fn republish_seed(case: &str, workflow: &mut Value) {
    if super::supported(case) && case.starts_with("publish-republish-") {
        workflow["is_published"] = json!(false);
        workflow["unpublished_change_count"] = json!(if case == "publish-republish-changed" {
            2
        } else {
            0
        });
    }
}
/// Constructed compatibility responses; no production version-reuse claim.
pub(super) fn published_case(case: &str, original: &Value) -> Value {
    let mut value = published(original);
    if matches!(
        case,
        "publish-republish-same-version" | "publish-republish-stale"
    ) {
        value["last_published_version_id"] = original["last_published_version_id"].clone();
    }
    value
}
pub(super) fn synchronized(original: &Value) -> Value {
    let mut value = original.clone();
    value["inputs"] = json!({"keep":original["inputs"]["keep"],"custom":{"customizable":true},"channel":{"value":"{{data.channel}}"}});
    value
}
pub(super) fn available() -> Value {
    json!({"{{steps.STEP_REVIEW.result}}":{"type":"string","name":"result","opaque":7}})
}
pub(super) fn expression() -> Value {
    json!({"opaque":{"keep":[1,"publication"]}})
}
pub(super) fn primary(workflow: &Value) -> Value {
    json!({"ok":true,"decorated_workflow":workflow,"available_data":available(),"expression_data":expression(),"opaque_receipt":{"preserve":[1,"publish"]}})
}
pub(super) fn sync(trigger: &Value) -> Value {
    json!({"ok":true,"trigger":trigger,"opaque_receipt":{"preserve":[2,"synchronize"]}})
}

pub(super) fn trigger_types(available: bool) -> Value {
    json!({"ok":true,"trigger_types":[{"trigger_type":{"id":"TT_SHORTCUT","type":"shortcut"},
        "is_available_to_user":available,"private_channel_access":false,"private_channel_team_ids":[]}]})
}
