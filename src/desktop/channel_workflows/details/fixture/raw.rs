//! Immutable original envelopes, deliberately different from list previews/bookmarks.
use serde_json::{Value, json};
pub(crate) fn workflow_id(trigger: &str) -> &'static str {
    if trigger == "TR_REVIEW" {
        "WfREVIEW01"
    } else {
        "WfFOLDER01"
    }
}
pub(crate) fn trigger(id: &str) -> Value {
    json!({"ok":true,"trigger":{"id":id,"type":"shortcut","name":if id=="TR_REVIEW"{"Info review \u{4e2d}"}else{""},"description":if id=="TR_REVIEW"{"Original trigger description"}else{""},"owning_team_id":"TWORKFLOW",
        "workflow":{"workflow_id":workflow_id(id),"title":if id=="TR_REVIEW"{"Inline workflow title"}else{""},"description":if id=="TR_REVIEW"{"Inline workflow description"}else{""},"app_id":"A_HOME","app":{"id":"A_HOME","name":"Review requests"}},
        "workflow_details":{"collaborators":["UWORKFLOW"],"steps":[{"id":"inline","type":"function","function":{"title":"Inline step must be replaced"}}]},"opaque":{"original":true,"order":[3,1]}},"opaque_receipt":"original-details-info"})
}
pub(crate) fn workflow(trigger: &str) -> Value {
    json!({"ok":true,"workflow":{"id":workflow_id(trigger),"title":if trigger=="TR_REVIEW"{"Full review \u{4e2d}"}else{"Folder details caf\u{e9}"},
        "description":if trigger=="TR_REVIEW"{"Actual complete workflow description \u{e9}"}else{"Full folder workflow description"},
        "collaborators":["UWORKFLOW"],"steps":[{"id":"step-original","type":"function","function":{"title":"Review the original request"}}],"opaque_original":{"retained":[false,7]}},"opaque_receipt":"original-details-workflow"})
}
