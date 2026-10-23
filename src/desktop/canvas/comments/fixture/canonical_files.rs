//! Named original embedded-file batches and fresh, disconnected file resolver.
use super::*;
pub(in crate::canvas) mod auto_refresh;
mod callback;
pub(in crate::canvas) mod cold;
pub(in crate::canvas) mod destination_canvas;
pub(in crate::canvas) mod known;
pub(in crate::canvas) mod resolver;
pub(in crate::canvas) mod return_body;
pub(in crate::canvas) use callback::{invoke_original, remember_model, remember_token};
pub(in crate::canvas) const FLAG: &str = "SUPER_PLATINUM_CANVAS_CANONICAL_FILE_CHECK";
pub(in crate::canvas) const ROOT: &str = "1719800000.000100";
pub(in crate::canvas) const ROOTS: [&str; 5] = [
    ROOT,
    "1719800020.000100",
    "1719800030.000100",
    "1719800040.000100",
    "1719800050.000100",
];
pub(in crate::canvas) const HOSTED: &str = "FCANON001";
pub(in crate::canvas) const CANVAS: &str = "FCANON002";
pub(in crate::canvas) const LIST: &str = "FCANON003";
pub(in crate::canvas) const BLOCKED: &str = "FCANON004";
pub(in crate::canvas) const SECOND: &str = "FCANON005";
pub(in crate::canvas) const URLS: [&str; 3] = [
    "https://example.test/canonical/discarded-file.png",
    "https://example.test/canonical/discarded-thumb.png",
    "https://example.test/canonical/discarded-attachment.png",
];
#[derive(Clone, Copy, Default)]
pub(in crate::canvas) enum Mode {
    #[default]
    Original,
    Renamed,
    NoPermalink,
    Deleted,
}
#[derive(Default)]
struct Record {
    mode: Mode,
    originals: Vec<Value>,
    retained: Value,
}
static CANONICAL: OnceLock<Mutex<Record>> = OnceLock::new();
pub(in crate::canvas) fn requested() -> bool {
    destinations_requested()
        || crate::fixture::is_fixture()
            && matches!(
                std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
                Ok("canvas-canonical-file-roots"
                    | "canvas-canonical-file-roots-narrow"
                    | "canvas-canonical-file-custody"
                    | "canvas-canonical-file-custody-narrow")
            )
}
pub(in crate::canvas) fn destination_id() -> Option<&'static str> {
    if !cfg!(any(test, feature = "protected-native-fixture")) || !crate::fixture::is_fixture() {
        return None;
    }
    match std::env::var("SUPER_PLATINUM_FIXTURE").as_deref() {
        Ok("canvas-canonical-native-canvas" | "canvas-canonical-native-canvas-narrow") => {
            Some(CANVAS)
        }
        Ok("canvas-canonical-native-list" | "canvas-canonical-native-list-narrow") => Some(LIST),
        _ => None,
    }
}
pub(in crate::canvas) fn destinations_requested() -> bool {
    destination_id().is_some()
}
pub(in crate::canvas) fn custody() -> bool {
    destinations_requested()
        || requested()
            && std::env::var("SUPER_PLATINUM_FIXTURE")
                .is_ok_and(|name| name.starts_with("canvas-canonical-file-custody"))
}
pub(in crate::canvas) fn enabled(shell: &ShellState) -> bool {
    requested()
        && shell.core.session.is_none()
        && shell.core.transport.is_none()
        && shell
            .canvas
            .as_ref()
            .is_some_and(|file| file.id == "Ffixture")
}
pub(in crate::canvas) fn blocks() -> Vec<Value> {
    vec![
        json!({"type":"rich_text","block_id":"canonical-draft","elements":[{"type":"rich_text_section","elements":[
            {"type":"text","text":"Keep exact \u{4e2d}\u{1f98a} & ","style":{"bold":true}},
            {"type":"link","url":"https://example.test/draft","text":"draft link","style":{"italic":true}},
            {"type":"text","text":" e\u{301}."}
        ]}]}),
        json!({"type":"rich_text","block_id":"opaque-draft","opaque":{"keep":[0,false,null,"\u{1f680}"]},
            "elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Opaque attributes retained.","fixture_opaque":{"zero":0,"enabled":false}}]}]}),
    ]
}
pub(in crate::canvas) fn install(shell: &mut ShellState) {
    if !requested() {
        return;
    }
    let id = shell.media.register_deferred(
        super_platinum_core::MediaAssetKind::Attachment,
        resolver::PREVIEW,
        "image/svg+xml",
    );
    let bytes: std::sync::Arc<[u8]> = std::sync::Arc::from(
        br##"<svg xmlns="http://www.w3.org/2000/svg" width="640" height="360" viewBox="0 0 640 360"><rect width="640" height="360" fill="#153e4b"/><text x="40" y="145" fill="white" font-size="32" font-family="sans-serif">Fresh canonical file preview</text><text x="40" y="205" fill="white" font-size="20" font-family="sans-serif">Local fixture bytes; no remote media</text></svg>"##.as_slice(),
    );
    shell.media.insert(id, "image/svg+xml", bytes);
}
pub(in crate::canvas) fn configure(shell: &ShellState, mode: Mode) -> Result<(), Error> {
    if !enabled(shell) {
        return Err(Error::Api("canonical_fixture_retired".into()));
    }
    CANONICAL.get_or_init(Default::default).lock().unwrap().mode = mode;
    Ok(())
}
pub(in crate::canvas) fn file(id: &str) -> Value {
    let (mode, kind, name) = match id {
        HOSTED => ("hosted", "svg", "Hosted canonical & \u{4e2d}\u{1f98a}"),
        CANVAS => ("quip", "quip", "Canvas canonical name"),
        LIST => ("list", "list", "List canonical name"),
        SECOND => ("hosted", "svg", "SECOND MUST NOT WIN"),
        _ => return json!({"id":id,"mode":"tombstone"}),
    };
    json!({"id":id,"mode":mode,"filetype":kind,"name":name,"title":format!("DIFFERENT title {id}"),"created":101,
        "permalink":format!("https://example.test/canonical/{id}"),"is_deleted":false,
        "url_private":URLS[0],"thumb_360":URLS[1],"fixture_opaque":{"retain":[false,0,null]}})
}
fn original(ts: &str, id: &str, label: &str, mode: Mode) -> Value {
    let mut row = root(ts, &format!("{label} fallback & \u{4e2d}\u{1f98a}"));
    row["document_comment"]["fixture_opaque"] = json!({"keep":true,"order":[2,1]});
    row["blocks"] = json!([{"type":"rich_text","block_id":format!("original-{ts}"),"elements":[{"type":"rich_text_section","elements":[{"type":"text","text":row["text"],"style":{"bold":true}}]}]}]);
    let mut embedded = file(id);
    embedded["name"] = json!(format!("EMBEDDED first {id}"));
    let mut canonical = file(id);
    if ts == ROOT {
        canonical = match mode {
            Mode::Original => canonical,
            Mode::Renamed => {
                json!({"id":id,"name":"Renamed canonical & \u{4e2d}\u{1f98a}","permalink":"https://example.test/canonical/renamed","updated":102})
            }
            Mode::NoPermalink => json!({"id":id,"name":"Name without permalink","updated":103}),
            Mode::Deleted => json!({"id":id,"is_deleted":true,"updated":104}),
        };
    }
    // Both source occurrences remain in original Message. D8Sp's independent
    // canonical batch uses the final same-ID raw object, against one preimage.
    row["files"] = json!([embedded, canonical]);
    row["attachments"] = json!([{"id":1,"text":"DISCARDED attachment text","image_url":URLS[2],"image_width":120,"image_height":80}]);
    row
}
pub(in crate::canvas) fn roots(mode: Mode) -> Vec<Value> {
    let mut rows = vec![
        original(ROOTS[0], destination_id().unwrap_or(HOSTED), "Hosted", mode),
        original(ROOTS[1], CANVAS, "Canvas", Mode::Original),
        original(ROOTS[2], LIST, "List", Mode::Original),
        original(ROOTS[3], SECOND, "Hidden", Mode::Original),
        original(ROOTS[4], BLOCKED, "First blocked", Mode::Original),
    ];
    rows[3]["document_comment"]["is_visible"] = json!(false);
    rows[4]["files"] = json!([file(BLOCKED), file(SECOND)]);
    cold::decorate(&mut rows);
    rows
}
pub(super) fn response(ticket: &Ticket) -> Result<Value, Error> {
    let mut record = CANONICAL.get_or_init(Default::default).lock().unwrap();
    let rows = roots(record.mode);
    let messages = match (&ticket.query, ticket.cursor.as_deref()) {
        (Query::Stream(Filter::Recent), None) => rows,
        (Query::Replies(ts), None) if ts == ROOT => vec![
            rows[0].clone(),
            reply(
                "1719800001.000100",
                ROOT,
                "Ordinary reply remains unchanged.",
            ),
        ],
        _ => return Err(Error::Api("canonical_fixture_query".into())),
    };
    record.originals = messages.clone();
    Ok(json!({"ok":true,"has_more":false,"messages":messages}))
}
pub(in crate::canvas) fn observe(shell: &ShellState, state: &State) {
    if !enabled(shell) {
        return;
    }
    let mut record = CANONICAL.get_or_init(Default::default).lock().unwrap();
    let normalized = |message: &Message| {
        let value = serde_json::to_value(message).unwrap();
        let matches = record
            .originals
            .iter()
            .find(|raw| raw["ts"].as_str() == message.ts.as_deref())
            .and_then(|raw| serde_json::from_value::<Message>(raw.clone()).ok())
            .and_then(|m| serde_json::to_value(m).ok())
            .as_ref()
            == Some(&value);
        json!({"message":value,"matchesOriginal":matches})
    };
    let rows = state
        .rows
        .iter()
        .map(|row| normalized(&row.message))
        .collect::<Vec<_>>();
    let root = state.root.as_ref().map(normalized);
    let query = match &state.query {
        Some(Query::Stream(_)) => json!({"kind":"stream","rootTs":null}),
        Some(Query::Replies(ts)) => json!({"kind":"thread","rootTs":ts}),
        None => Value::Null,
    };
    record.retained = json!({"query":query,"serial":state.serial,"ownerCurrent":state.owner.as_ref().is_some_and(|o|o.current(shell)),
        "loading":state.loading,"error":state.error,"rows":rows,"root":root,
        "pendingFile":state.pending_file.is_some(),"navigationError":state.mutation_error,
        "catalog":super::super::file_source::snapshot(shell,state)});
    if known::requested() {
        record.retained["knownAcquisition"] =
            super::super::file_acquisition::snapshot(shell, state);
    }
    if cold::requested() {
        record.retained["coldAcquisition"] = super::super::file_acquisition::snapshot(shell, state);
    }
}
pub(in crate::canvas) fn proof(shell: &ShellState) -> Value {
    if !requested() {
        return Value::Null;
    }
    let (retained, originals) = {
        let record = CANONICAL.get_or_init(Default::default).lock().unwrap();
        (record.retained.clone(), record.originals.clone())
    };
    let mut proof = json!({"requests":ledger(),"responses":responses(),"retained":retained,
        "originals":originals,"resolver":resolver::snapshot(),
        "media":URLS.iter().map(|url|json!({"url":url,"registered":shell.media.registered_source_count(url)})).collect::<Vec<_>>()});
    if known::requested() {
        proof["known"] = known::snapshot();
    }
    if cold::requested() {
        proof["cold"] = cold::snapshot();
    }
    proof
}
pub(in crate::canvas) async fn resolve(
    state: Signal<ShellState>,
    id: &str,
) -> Result<Value, Error> {
    resolver::resolve(state, id).await
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn durable_rich_seed_accepts_opaque_attributes_without_unknown_nodes() {
        let blocks = blocks();
        assert!(super_platinum_core::canvas::comments::content::fallback(&blocks).is_ok());
        assert_eq!(
            blocks[0]["elements"][0]["elements"][0]["style"],
            json!({"bold":true})
        );
        assert_eq!(blocks[1]["opaque"]["keep"][0], 0);
        assert_eq!(blocks[1]["opaque"]["keep"][1], false);
    }
    #[test]
    fn original_occurrences_and_final_raw_partial_survive_typed_message() {
        let roots = roots(Mode::NoPermalink);
        let message: Message = serde_json::from_value(roots[0].clone()).unwrap();
        assert_eq!(message.files.len(), 2);
        assert_eq!(message.files[0].id, message.files[1].id);
        assert!(roots[0]["files"][1].get("permalink").is_none());
        assert!(roots[0]["files"][1].get("mode").is_none());
        assert_eq!(roots[4]["files"][0]["id"], BLOCKED);
        assert_eq!(roots[4]["files"][1]["id"], SECOND);
    }
}
