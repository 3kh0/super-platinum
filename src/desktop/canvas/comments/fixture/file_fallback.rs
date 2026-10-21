//! Synthetic embedded HTTP file objects; no canonical generic-file authority.
use super::*;
pub(in crate::canvas) const ROOT: &str = "1719800000.000100";
pub(in crate::canvas) const CONTROLS: [&str; 6] = [
    "1719800020.000100",
    "1719800030.000100",
    "1719800040.000100",
    "1719800050.000100",
    "1719800060.000100",
    "1719800070.000100",
];
pub(in crate::canvas) const URLS: [&str; 7] = [
    "https://example.test/file-fallback/file.png",
    "https://example.test/file-fallback/download.png",
    "https://example.test/file-fallback/thumb.png",
    "https://example.test/file-fallback/attachment.png",
    "https://example.test/file-fallback/attachment-thumb.png",
    "https://example.test/file-fallback/author.png",
    "https://example.test/file-fallback/second.png",
];
pub(in crate::canvas) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-file-fallback" | "canvas-file-fallback-narrow")
        )
}
pub(in crate::canvas) fn enabled(shell: &ShellState) -> bool {
    requested() && eligible(shell)
}
fn decoys(mut row: Value) -> Value {
    row["files"] = json!([{"id":"FFALLBACK1","name":"DECOY file name","title":"DECOY file title","filetype":"png","mimetype":"image/png","size":42,"url_private":URLS[0],"url_private_download":URLS[1],"thumb_360":URLS[2],"original_w":120,"original_h":80}]);
    row["attachments"] = json!([{"id":1,"text":"DECOY attachment","title":"DECOY attachment title","image_url":URLS[3],"image_width":120,"image_height":80,"thumb_url":URLS[4],"author_name":"DECOY attachment author","author_icon":URLS[5]}]);
    row["document_comment"]["opaque"] = json!({"preserve":true});
    row
}
pub(in crate::canvas) fn primary() -> Value {
    let mut row = decoys(root(ROOT, "DECOY primary fallback"));
    row["document_comment"]["is_visible"] = json!(true);
    row["blocks"] = json!([{"type":"rich_text","block_id":"original-block","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Original block & \u{4e2d}\u{1f98a}","style":{"bold":true,"underline":true}}]}]}]);
    row["files"].as_array_mut().unwrap().push(json!({"id":"FFALLBACK2","name":"DECOY second file","title":"DECOY second title","filetype":"png","mimetype":"image/png","url_private":URLS[6]}));
    row
}
pub(in crate::canvas) fn controls() -> Vec<Value> {
    let mut rows: Vec<Value> = CONTROLS
        .into_iter()
        .map(|ts| decoys(root(ts, "")))
        .collect();
    rows[0]["text"] = json!("DECOY unsupported fallback");
    rows[0]["blocks"] = json!([{"type":"fixture_unsupported","text":"DECOY unsupported block"}]);
    rows[1]["text"] = json!("_Formatted &amp; \u{4e2d}\u{1f98a} _");
    rows[1]["blocks"] = json!([]);
    rows[2]["markdown_text"] = json!("*Markdown &amp; \u{4e2d}\u{1f98a}*");
    rows[2]["document_comment"]["is_visible"] = Value::Null;
    rows[3]["text"] = json!("*Literal &amp; \u{4e2d}\u{1f98a}*");
    rows[3]["mrkdwn"] = json!(false);
    rows[4]["text"] = json!("DECOY hidden text");
    rows[4]["document_comment"]["is_visible"] = json!(false);
    rows[4]["blocks"] =
        json!([{"type":"section","text":{"type":"plain_text","text":"DECOY hidden blocks"}}]);
    rows[5]["markdown_text"] = json!({"unsupported":"DECOY malformed fallback"});
    rows
}
pub(super) fn response(ticket: &Ticket) -> Result<Value, Error> {
    match (&ticket.query, ticket.cursor.as_deref()) {
        (Query::Stream(Filter::Recent), None) => Ok(
            json!({"ok":true,"has_more":true,"response_metadata":{"next_cursor":"fallback-controls"},"messages":[primary()]}),
        ),
        (Query::Stream(Filter::Recent), Some("fallback-controls")) => {
            Ok(json!({"ok":true,"has_more":false,"messages":controls()}))
        }
        (Query::Replies(ts), None) if ts == ROOT => Ok(
            json!({"ok":true,"has_more":false,"messages":[primary(),reply("1719800001.000100",ROOT,"Ordinary reply remains unchanged.")]}),
        ),
        _ => Err(Error::Api("fixture_unexpected_file_fallback_query".into())),
    }
}
pub(in crate::canvas) fn proof(shell: &ShellState) -> Value {
    if !enabled(shell) {
        return Value::Null;
    }
    let retained = RECORD
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .file_fallback_retained
        .clone();
    json!({"primary":primary(),"controls":controls(),"requests":ledger(),"responses":responses(),"retained":retained,"media":URLS.iter().map(|url|json!({"url":url,"registered":shell.media.registered_source_count(url)})).collect::<Vec<_>>()})
}
fn normalized_original(ts: Option<&str>) -> Option<Value> {
    let source = std::iter::once(primary())
        .chain(controls())
        .chain(std::iter::once(reply(
            "1719800001.000100",
            ROOT,
            "Ordinary reply remains unchanged.",
        )))
        .find(|raw| raw["ts"].as_str() == ts)?;
    serde_json::to_value(serde_json::from_value::<Message>(source).ok()?).ok()
}
fn retained_snapshot(shell: &ShellState, state: &State) -> Value {
    let query = match &state.query {
        Some(Query::Stream(_)) => json!({"kind":"stream","rootTs":null}),
        Some(Query::Replies(ts)) => json!({"kind":"thread","rootTs":ts}),
        None => Value::Null,
    };
    json!({"query":query,"serial":state.serial,"ownerCurrent":state.owner.as_ref().is_some_and(|o|o.current(shell)),"loading":state.loading,"error":state.error,"root":state.root,
 "rows":state.rows.iter().map(|row|{let message=serde_json::to_value(&row.message).expect("fixture retained Message serializes");let matches=normalized_original(row.message.ts.as_deref()).as_ref()==Some(&message);json!({"message":message,"matchesOriginal":matches})}).collect::<Vec<_>>()})
}
pub(in crate::canvas) fn observe(shell: &ShellState, state: &State) {
    if !enabled(shell) {
        return;
    }
    let snapshot = retained_snapshot(shell, state);
    RECORD
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .file_fallback_retained = snapshot;
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn raw_vectors_deserialize_without_normalizing_file_order_or_malformed_fallback() {
        let raw = primary();
        let message: Message = serde_json::from_value(raw).unwrap();
        assert_eq!(message.files.len(), 2);
        assert_eq!(message.files[0].id.as_deref(), Some("FFALLBACK1"));
        assert_eq!(message.files[1].id.as_deref(), Some("FFALLBACK2"));
        let rows = controls();
        for raw in &rows {
            let _: Message = serde_json::from_value(raw.clone()).unwrap();
        }
        assert!(rows[1]["document_comment"].get("is_visible").is_none());
        assert!(rows[2]["document_comment"]["is_visible"].is_null());
        assert!(rows[5]["markdown_text"].is_object());
    }
}
