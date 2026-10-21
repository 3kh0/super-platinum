//! Independent immutable wire rows; no ready-view or permission seed.
use serde_json::{Value, json};
pub(crate) const DEST: &str = "CADDDEST01";
pub(crate) const PRIVATE: &str = "CADDPRIV01";
pub(crate) const IDS: [&str; 3] = ["TR_REVIEW", "FtDenied01", "FtFolder01"];
pub(crate) fn channel(row: &mut Value) {
    if row["id"] == DEST {
        row["properties"] = json!({"channel_workflows":[
            {"workflow_trigger_id":"TR_REVIEW","title":"Featured override unused"},
            {"workflow_trigger_id":"FtDenied01","title":"Private featured \u{4e2d}"}
        ]});
        if super::endpoint::case() == "first-tab-rate-limit"
            || crate::channel_workflows::details::fixture::thread::requested()
        {
            row["properties"]["tabz"] = json!([{"id":"workflows","type":"workflows","label":""}]);
        }
    } else if row["id"] == PRIVATE {
        row["properties"] = json!({"channel_workflows":[],"tabz":[{"id":"workflows","type":"workflows","label":""}]});
    }
}
pub(crate) fn bookmarks(channel: &str, accepted: &[Value]) -> Value {
    if channel == PRIVATE {
        return json!({"ok":true,"bookmarks":[],"opaque_receipt":"original-empty-workflows"});
    }
    let mut rows = accepted.to_vec();
    rows.extend([
        json!({"id":"BkFolder01","channel_id":DEST,"type":"auto_gen_shortcut","shortcut_id":"FtFolder01","title":"Untrusted bookmark title","parent_id":"BkParent01","rank":"0"}),
        json!({"id":"BkDenied01","channel_id":DEST,"type":"shortcut","shortcut_id":"FtDenied01","title":"Not private title authority","rank":"b"}),
        json!({"id":"BkDuplicate01","channel_id":DEST,"type":"shortcut","shortcut_id":"TR_REVIEW","title":"Duplicate later","rank":"z"}),
        json!({"id":"BkLegacy01","channel_id":DEST,"type":"legacy_workflow","shortcut_id":"FtLegacy01","title":"Legacy excluded","rank":"-1"}),
        json!({"id":"BkLink01","channel_id":DEST,"type":"link","link":"https://example.test/ordinary","title":"Ordinary excluded","rank":"-2"})
    ]);
    json!({"ok":true,"bookmarks":rows,"opaque_receipt":"original-workflows-bookmarks"})
}
pub(crate) fn previews() -> Value {
    json!({"ok":true,"triggers":[
        {"id":"TR_REVIEW","type":"shortcut","name":"Published review \u{4e2d}","description":"Actual preview description \u{e9}","workflow":{"workflow_id":"WfREVIEW01","app_id":"A_HOME"},"opaque":{"retained":[3,1]}},
        {"id":"FtDenied01","error":"access_denied","opaque":"original-private"},
        {"id":"FtFolder01","type":"shortcut","name":"","description":"","workflow":{"workflow_id":"WfFOLDER01","title":"Folder workflow caf\u{e9}","description":"Actual workflow fallback"}}
    ],"opaque_receipt":"original-trigger-previews"})
}
