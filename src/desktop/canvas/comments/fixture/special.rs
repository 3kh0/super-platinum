//! Explicit synthetic embedded-file HTTP roots; no normalized-ID/fetch claim.
use super::*;

pub(in crate::canvas) const HIDDEN: &str = "1719800000.000100";
pub(in crate::canvas) const CONTROLS: [&str; 3] = [
    "1719800020.000100",
    "1719800030.000100",
    "1719800040.000100",
];
pub(in crate::canvas) const URLS: [&str; 7] = [
    "https://example.test/hidden-root/block.png",
    "https://example.test/hidden-root/file.png",
    "https://example.test/hidden-root/download.png",
    "https://example.test/hidden-root/thumb.png",
    "https://example.test/hidden-root/attachment.png",
    "https://example.test/hidden-root/attachment-thumb.png",
    "https://example.test/hidden-root/author.png",
];
pub(in crate::canvas) fn requested() -> bool {
    crate::fixture::is_fixture()
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("canvas-special-roots" | "canvas-special-roots-narrow")
        )
}
pub(in crate::canvas) fn enabled(shell: &ShellState) -> bool {
    requested() && eligible(shell)
}
pub(in crate::canvas) fn hidden() -> Value {
    let mut row = root(HIDDEN, "DECOY hidden fallback");
    row["markdown_text"] = json!("DECOY hidden markdown");
    row["document_comment"]["is_visible"] = json!(false);
    row["document_comment"]["most_recent_editor"] = json!("U2");
    row["document_comment"]["opaque"] = json!({"preserve":true});
    row["blocks"] = json!([
        {"type":"section","text":{"type":"plain_text","text":"DECOY hidden blocks"}},
        {"type":"image","image_url":URLS[0],"alt_text":"DECOY hidden image"}
    ]);
    row["files"] = json!([{"id":"FHIDDEN001","name":"DECOY hidden file","title":"DECOY hidden title",
        "filetype":"png","mimetype":"image/png","size":42,"url_private":URLS[1],
        "url_private_download":URLS[2],"thumb_360":URLS[3],"original_w":120,"original_h":80}]);
    row["attachments"] = json!([{"id":1,"text":"DECOY hidden attachment","title":"DECOY hidden attachment title",
        "title_link":"https://example.test/hidden-root/attachment-link","image_url":URLS[4],"image_width":120,"image_height":80,
        "thumb_url":URLS[5],"author_name":"DECOY hidden attachment author","author_icon":URLS[6]}]);
    row
}
pub(in crate::canvas) fn controls() -> Vec<Value> {
    CONTROLS.into_iter().zip(["Visible true", "Visibility absent", "Visibility null"]).enumerate().map(|(i,(ts,label))| {
        let mut row = root(ts,label);
        if i==0 { row["document_comment"]["is_visible"]=json!(true); }
        if i==2 { row["document_comment"]["is_visible"]=Value::Null; }
        row["blocks"] = json!([{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
            {"type":"text","text":format!("{label} & \u{4e2d}\u{1f98a}"),"style":{"bold":true,"underline":true}}
        ]}]}]);
        row["files"] = json!([{"id":format!("FCONTROL0{i}"),"name":format!("{label} file"),"title":format!("{label} file"),"filetype":"pdf","mimetype":"application/pdf"}]);
        row
    }).collect()
}
pub(super) fn response(ticket: &Ticket) -> Result<Value, Error> {
    match (&ticket.query, ticket.cursor.as_deref()) {
        (Query::Stream(Filter::Recent), None) => Ok(
            json!({"ok":true,"has_more":true,"response_metadata":{"next_cursor":"special-controls"},"messages":[hidden()]}),
        ),
        (Query::Stream(Filter::Recent), Some("special-controls")) => {
            Ok(json!({"ok":true,"has_more":false,"messages":controls()}))
        }
        (Query::Replies(ts), None) if ts == HIDDEN => Ok(
            json!({"ok":true,"has_more":false,"messages":[hidden(),reply("1719800001.000100",HIDDEN,"Visible reply remains readable.")]}),
        ),
        _ => Err(Error::Api("fixture_unexpected_special_root_query".into())),
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
        .special_retained
        .clone();
    json!({"hidden":hidden(),"controls":controls(),"requests":ledger(),"responses":responses(),"retained":retained,
        "media":URLS.iter().map(|url|json!({"url":url,"registered":shell.media.registered_source_count(url)})).collect::<Vec<_>>()})
}

fn normalized_original(ts: Option<&str>) -> Option<Value> {
    let source = std::iter::once(hidden())
        .chain(controls())
        .chain(std::iter::once(reply(
            "1719800001.000100",
            HIDDEN,
            "Visible reply remains readable.",
        )))
        .find(|raw| raw["ts"].as_str() == ts)?;
    let message: Message = serde_json::from_value(source).ok()?;
    serde_json::to_value(message).ok()
}
fn retained_snapshot(shell: &ShellState, state: &State) -> Value {
    let query = match &state.query {
        Some(Query::Stream(_)) => json!({"kind":"stream","rootTs":null}),
        Some(Query::Replies(ts)) => json!({"kind":"thread","rootTs":ts}),
        None => Value::Null,
    };
    json!({"query":query,"serial":state.serial,"ownerCurrent":state.owner.as_ref().is_some_and(|owner|owner.current(shell)),
        "loading":state.loading,"error":state.error,"root":state.root,
        "rows":state.rows.iter().map(|row|{
            let message=serde_json::to_value(&row.message).expect("fixture retained Message serializes");
            let matches=normalized_original(row.message.ts.as_deref()).as_ref()==Some(&message);
            json!({"message":message,"matchesOriginal":matches})
        }).collect::<Vec<_>>()})
}
/// Passive diagnostics of the actual mounted model. No signal writes or I/O;
/// ordinary renders return before any serialization or registry lock.
pub(in crate::canvas) fn observe(shell: &ShellState, state: &State) {
    if !enabled(shell) {
        return;
    }
    let snapshot = retained_snapshot(shell, state);
    RECORD
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .special_retained = snapshot;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retained_model_snapshot_detects_hidden_source_mutation_independent_of_display() {
        let shell = ShellState::fixture(crate::media::MediaRegistry::default());
        let message: Message = serde_json::from_value(hidden()).unwrap();
        let body = projection::message_vm(&shell.core.workspaces["T1"], &message, &shell.media);
        let mut state = State::default();
        state.serial = 41;
        state.query = Some(Query::Replies(HIDDEN.into()));
        state.root = Some(message.clone());
        state.rows.push(Row {
            message,
            body,
            preview: None,
            preview_body: None,
        });
        let original = retained_snapshot(&shell, &state);
        assert_eq!(original["serial"], 41);
        assert_eq!(original["query"]["rootTs"], HIDDEN);
        assert_eq!(original["rows"][0]["matchesOriginal"], true);
        assert_eq!(original["root"], original["rows"][0]["message"]);
        state.rows[0].message.files.clear();
        let changed = retained_snapshot(&shell, &state);
        assert_eq!(changed["rows"][0]["matchesOriginal"], false);
        assert_eq!(changed["rows"][0]["message"]["files"], json!([]));
        assert_eq!(changed["root"], original["root"]);
    }
    #[test]
    fn special_fixture_preserves_distinct_visibility_inputs_and_embedded_decoys() {
        let raw = hidden();
        let parsed: Message = serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(parsed.files.len(), 1);
        assert_eq!(parsed.attachments.len(), 1);
        assert_eq!(
            parsed.extra["document_comment"]["thread_id"],
            json!(format!("quip-{HIDDEN}"))
        );
        assert_eq!(raw["document_comment"]["is_visible"], false);
        let controls = controls();
        assert_eq!(controls[0]["document_comment"]["is_visible"], true);
        assert!(controls[1]["document_comment"].get("is_visible").is_none());
        assert!(controls[2]["document_comment"]["is_visible"].is_null());
        for row in controls {
            assert_eq!(
                serde_json::from_value::<Message>(row).unwrap().files.len(),
                1
            );
        }
    }
}
