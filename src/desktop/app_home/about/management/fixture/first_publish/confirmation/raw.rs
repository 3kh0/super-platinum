//! Immutable ordinary channel and shortcut-bookmark wire literals.
use super::*;
pub(crate) const DEST: &str = "CADDDEST01";
pub(crate) const PRIVATE: &str = "CADDPRIV01";
pub(crate) fn channels() -> Vec<Value> {
    let base = |id: &str, name: &str, private: bool| {
        json!({
            "id":id,"name":name,"context_team_id":"TWORKFLOW","is_channel":true,
            "is_private":private,"is_member":true,"is_archived":false,"is_im":false,"is_mpim":false,
            "is_general":false,"is_ext_shared":false,"is_org_shared":false,"is_shared":false,
            "is_read_only":false,"is_thread_only":false,
            "is_frozen":false,"is_huddle":false,"isUnknown":false,"isNonExistent":false
        })
    };
    let mut archived = base("CADDARCH01", "archived-review", false);
    archived["is_archived"] = json!(true);
    let mut nonmember = base("CADDLEFT01", "left-review", false);
    nonmember["is_member"] = json!(false);
    let mut denied = base("CADDREAD01", "read-only-review", false);
    denied["is_read_only"] = json!(true);
    denied["is_general"] = json!(true);
    let mut rows = vec![
        base(DEST, "review-destination", false),
        base(PRIVATE, "private-review", true),
        archived,
        nonmember,
        denied,
    ];
    if crate::channel_workflows::fixture::is_case(&super::case()) {
        for row in &mut rows {
            crate::channel_workflows::fixture::raw::channel(row);
        }
    }
    rows
}
pub(super) fn channel(id: &str) -> Option<Value> {
    channels().into_iter().find(|v| v["id"] == id)
}
pub(super) fn bookmark(trigger: &Value) -> Value {
    json!({"id":"BkFIRST01","channel_id":DEST,"type":"shortcut","shortcut_id":trigger["id"],
        "title":trigger["name"],"link":trigger["shortcut_url"],"rank":"a","opaque_original":{"retained":true}})
}
pub(super) fn rejection() -> Value {
    json!({"ok":false,"error":"not_allowed","opaque_receipt":"original-shortcut-rejected"})
}
