//! Closed file-title reads through the production Profile request boundary.
use super::*;
use serde_json::json;
use std::sync::{Mutex, OnceLock};
use super_platinum_core::slack::{
    Error,
    client::{PreparedRequest, RequestBody},
};

pub(crate) const FLAG: &str = "SUPER_PLATINUM_PROFILE_FILE_TITLES_CHECK";
pub(crate) const BLOCKS_FLAG: &str = "SUPER_PLATINUM_PROFILE_FILE_TITLE_BLOCKS";
pub(crate) const ID: &str = "FTITLE0001";
pub(crate) const URL: &str = "https://fixture.slack.com/docs/FTITLE0001";
#[derive(Clone, Copy, Default, PartialEq)]
pub(crate) enum Mode {
    #[default]
    Normal,
    Renamed,
    Offline,
    Held,
}
#[derive(Default)]
struct Ledger {
    mode: Mode,
    requests: Vec<Value>,
    completed: usize,
    cancelled: usize,
    held: Option<tokio::sync::oneshot::Sender<Value>>,
}
fn ledger() -> &'static Mutex<Ledger> {
    static LEDGER: OnceLock<Mutex<Ledger>> = OnceLock::new();
    LEDGER.get_or_init(Default::default)
}
pub(crate) fn named() -> bool {
    (super::mentions::named() && std::env::var(FLAG).as_deref() == Ok("1")) || mixed_group()
}
fn mixed_group() -> bool {
    (super::group_recovery::enabled() && super::group_recovery::case() == "mixed-file-group")
        || super::group_recovery::mixed_absent::enabled()
}
pub(crate) fn enabled(shell: &ShellState) -> bool {
    named() && super::mentions::enabled(shell)
}
pub(crate) fn list() -> bool {
    mixed_group()
        || std::env::var("SUPER_PLATINUM_PROFILE_FILE_TITLES_KIND").as_deref() == Ok("list")
}
pub(crate) fn block_mode() -> bool {
    !list() && std::env::var(BLOCKS_FLAG).as_deref() == Ok("1")
}
pub(crate) fn kind() -> &'static str {
    if list() { "file" } else { "canvas" }
}
pub(crate) fn source_label() -> &'static str {
    if block_mode() {
        "Misleading Canvas fallback"
    } else if list() {
        "  List &amp; Plan  "
    } else {
        "Canvas &amp; Plan"
    }
}
pub(crate) fn mounted_label() -> &'static str {
    if block_mode() {
        "&amp; *literal*"
    } else if list() {
        "List & Plan"
    } else {
        "Canvas & Plan"
    }
}
pub(crate) fn renamed_label() -> &'static str {
    if block_mode() {
        "Changed &amp;"
    } else if list() {
        "Renamed List"
    } else {
        "Renamed Canvas"
    }
}
pub(crate) fn raw(renamed: bool) -> Value {
    let mut raw = json!({"ok":true,"file":{"id":ID,"mode":if list(){"list"}else{"quip"},
        "filetype":if list(){"list"}else{"quip"},"created":101,"updated":if renamed{102}else{101},
        "title":if renamed{renamed_label()}else{source_label()},"name":"wrong-name-fallback",
        "fixture_opaque":{"preserve":[ID,1,2]}}});
    if block_mode() {
        raw["file"]["title"] = json!(if renamed {
            "Misleading renamed fallback"
        } else {
            source_label()
        });
        raw["file"]["title_blocks"] = title_blocks(renamed);
    }
    raw
}
/// Actual block wire data, deliberately distinct from the file's plain title.
/// Formatting and unknown fields survive in source/effective metadata; the
/// canonical title extractor treats these text values literally.
pub(crate) fn title_blocks(renamed: bool) -> Value {
    let values = if renamed {
        [":memo: ", "Changed ", "&amp;"]
    } else {
        [":wave: ", "&amp; ", "*literal*"]
    };
    json!([{"type":"rich_text","block_id":"file-title-block",
    "opaque_block":{"retain":["block",7]},"elements":[{
        "type":"rich_text_section","opaque_section":{"retain":true},"elements":[
            {"type":"text","text":values[0],"opaque_run":{"index":0}},
            {"type":"text","text":values[1],"style":{"italic":true},"opaque_run":{"index":1}},
            {"type":"text","text":values[2],"style":{"bold":true},"opaque_run":{"index":2}}
        ]}]}])
}
pub(crate) fn blocks(text: &str) -> Vec<Value> {
    let token = |occurrence, style| {
        json!({"type":kind(),"file_id":ID,"text":URL,"url":URL,
        "fixture_occurrence":occurrence,"style":style,"opaque":{"keep":[occurrence,"original"]}})
    };
    vec![
        json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[
        {"type":"text","text":text},token(1,json!({"bold":true})),token(2,json!({"italic":true}))]}]}),
    ]
}
pub(crate) fn install() {
    *ledger().lock().unwrap() = Ledger::default();
}
pub(crate) fn mode(mode: Mode) {
    ledger().lock().unwrap().mode = mode;
}
pub(crate) fn status() -> Value {
    let l = ledger().lock().unwrap();
    json!({"requests":l.requests,"completed":l.completed,"cancelled":l.cancelled,
        "held":l.held.is_some(),"active":l.requests.len()-l.completed,"networkCalls":0})
}
pub(crate) fn release() -> Value {
    let held = ledger().lock().unwrap().held.take();
    let cancelled = held.as_ref().is_some_and(|s| s.is_closed());
    let delivered = held.is_some_and(|s| s.send(raw(false)).is_ok());
    json!({"delivered":delivered,"cancelledBeforeReply":cancelled})
}
fn unavailable() -> Error {
    Error::Api("closed_profile_file_request_unavailable".into())
}
fn request_url(raw: &str) -> Option<(String, Vec<(String, String)>)> {
    let mut url = url::Url::parse(raw).ok()?;
    let mut pairs = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for (key, value) in url.query_pairs() {
        if !seen.insert(key.to_string()) {
            return None;
        }
        let value = if matches!(key.as_ref(), "_x_id" | "_x_csid" | "fp") {
            if value.len() != 32 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
                return None;
            }
            "<uuid>".to_owned()
        } else {
            value.to_string()
        };
        pairs.push((key.to_string(), value));
    }
    url.set_query(None);
    Some((url.to_string(), pairs))
}
fn validate(actual: &PreparedRequest, expected: &PreparedRequest) -> bool {
    actual.method == expected.method
        && request_url(&actual.url).is_some()
        && request_url(&actual.url) == request_url(&expected.url)
        && actual.headers == expected.headers
        && matches!((&actual.body,&expected.body),(RequestBody::Form(a),RequestBody::Form(b)) if a==b)
}
struct Completion(bool);
impl Drop for Completion {
    fn drop(&mut self) {
        let mut l = ledger().lock().unwrap();
        l.completed += 1;
        if !self.0 {
            l.cancelled += 1;
        }
    }
}
pub(crate) async fn read(
    state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Result<Value, Error> {
    if super::completion::files::enabled(&state.peek()) {
        let shell = state.peek();
        if url::Url::parse(&request.url)
            .ok()
            .is_none_or(|url| url.path() != "/api/files.info")
        {
            return Err(unavailable());
        }
        return super::completion::request(&shell, request).map(|(raw, _)| raw);
    }
    {
        let shell = state.peek();
        if !enabled(&shell) {
            return Err(unavailable());
        }
        let context = crate::scheduled::Context::capture(&shell).ok_or_else(unavailable)?;
        let expected = context.client.rest_form(
            &context.session,
            "files.info",
            vec![
                ("file", ID.into()),
                ("page", "1".into()),
                ("count", "500".into()),
                ("truncate", "true".into()),
            ],
        );
        if !validate(request, &expected) {
            return Err(unavailable());
        }
    }
    let (mode, receiver) = {
        let mut l = ledger().lock().unwrap();
        if l.held.is_some() {
            return Err(unavailable());
        }
        let serial = l.requests.len() + 1;
        l.requests.push(json!({"serial":serial,"method":"files.info","args":{"file":ID,"page":"1","count":"500","truncate":"true"}}));
        let receiver = if l.mode == Mode::Held {
            let (tx, rx) = tokio::sync::oneshot::channel();
            l.held = Some(tx);
            Some(rx)
        } else {
            None
        };
        (l.mode, receiver)
    };
    let mut completed = Completion(false);
    let result = match mode {
        Mode::Held => receiver.unwrap().await.map_err(|_| unavailable()),
        Mode::Offline => Err(Error::Offline("closed file-title fixture".into())),
        Mode::Normal => Ok(raw(false)),
        Mode::Renamed => Ok(raw(true)),
    };
    completed.0 = true;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn closed_request_requires_exact_factory_bytes() {
        let mut request = PreparedRequest {
            method: "POST",
            url: "https://fixture.invalid/api/files.info".into(),
            headers: vec![],
            body: RequestBody::Form(vec![
                ("file".into(), ID.into()),
                ("page".into(), "1".into()),
            ]),
        };
        let expected = request.clone();
        assert!(validate(&request, &expected));
        request.method = "GET";
        assert!(!validate(&request, &expected));
        request = expected.clone();
        request.url.push_str("/other");
        assert!(!validate(&request, &expected));
        request = expected.clone();
        request.body = RequestBody::Form(vec![("file".into(), "FFOREIGN".into())]);
        assert!(!validate(&request, &expected));
    }
    #[test]
    fn duplicate_file_tokens_keep_wire_and_distinct_styles() {
        let raw = blocks("draft");
        let values = &raw[0]["elements"][0]["elements"];
        assert_eq!(values[1]["file_id"], ID);
        assert_eq!(values[2]["file_id"], ID);
        assert_eq!(values[1]["text"], URL);
        assert_eq!(values[2]["url"], URL);
        assert_ne!(values[1]["style"], values[2]["style"]);
        assert_eq!(values[1]["opaque"]["keep"][0], 1);
        assert_eq!(values[2]["opaque"]["keep"][0], 2);
        assert_ne!(mounted_label().encode_utf16().count(), ID.len());
    }
    #[test]
    fn title_blocks_preserve_split_literal_runs_and_opaque_style() {
        for (renamed, expected) in [
            (false, ":wave: &amp; *literal*"),
            (true, ":memo: Changed &amp;"),
        ] {
            let raw = title_blocks(renamed);
            let runs = raw[0]["elements"][0]["elements"].as_array().unwrap();
            assert_eq!(
                runs.iter()
                    .map(|run| run["text"].as_str().unwrap())
                    .collect::<String>(),
                expected
            );
            assert_eq!(raw[0]["opaque_block"]["retain"], json!(["block", 7]));
            assert_eq!(raw[0]["elements"][0]["opaque_section"]["retain"], true);
            assert_eq!(runs[1]["style"], json!({"italic":true}));
            assert_eq!(runs[2]["style"], json!({"bold":true}));
            for (index, run) in runs.iter().enumerate() {
                assert_eq!(run["opaque_run"]["index"], index);
            }
        }
    }
}
