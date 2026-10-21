//! Original closed-server values; local proposed metadata is never applied here.
use super::*;
pub(super) fn seed(workflow: &mut Value, trigger: &mut Value) {
    workflow["is_published"] = json!(true);
    workflow["input_parameters"] = json!({"keep":{},"removed":{},"custom":{}});
    trigger["inputs"] = json!({"keep":{"value":"literal source","opaque":{"preserve":[1,true,"original"]}},"removed":{"value":"remove"},"custom":false});
    trigger["available_data"] = json!({
        "{{data.custom}}":{"name":"custom","type":"string","title":"Custom","is_customizable":true,"description":"Original custom input"},
        "{{data.channel}}":{"name":"channel","type":"slack#/types/channel_id","is_customizable":false,"title":"Channel"}
    });
}
pub(super) fn unpublished(original: &Value) -> Value {
    let mut value = original.clone();
    value["is_published"] = json!(false);
    value["date_updated"] = json!(2);
    value["input_parameters"] = json!({"keep":{},"custom":{},"channel":{}});
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
    json!({"ok":true,"decorated_workflow":workflow,"available_data":available(),"expression_data":expression(),"opaque_receipt":{"preserve":[1,"unpublish"]}})
}
pub(super) fn sync(trigger: &Value) -> Value {
    json!({"ok":true,"trigger":trigger,"opaque_receipt":{"preserve":[2,"synchronize"]}})
}
