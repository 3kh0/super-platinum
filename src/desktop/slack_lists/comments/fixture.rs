//! Closed raw comment acquisition; no transport or session is installed.
use super::*;
use serde_json::json;
use std::{collections::BTreeMap, sync::Mutex};
use super_platinum_core::{
    config::WorkspaceSession,
    slack::client::{PreparedRequest, RequestBody},
};
use tokio::sync::oneshot;
pub(crate) mod compose;

pub(crate) const LIST: &str = "FCOMMENT01";
pub(crate) const ROOT: &str = "1781524800.000001";
pub(crate) const SYNTHETIC_ROOT: &str = "1781524900.000001";
pub(crate) const CURSOR: &str = "comments-next+/=";
pub(crate) fn check_enabled() -> bool {
    (std::env::var("SUPER_PLATINUM_LIST_COMMENTS_CHECK").as_deref() == Ok("1")
        || compose::check_enabled())
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("slack-list" | "slack-list-narrow")
        )
}
pub(crate) fn enabled(s: &ShellState) -> bool {
    check_enabled()
        && crate::slack_lists::fixture::enabled(s)
        && s.core.session.is_none()
        && s.core.transport.is_none()
        && s.slack_lists.comments.fixture.is_some()
}
pub(crate) fn seed(document: &mut Value, records: &mut Vec<Value>) {
    if !check_enabled() {
        return;
    }
    document["id"] = json!(LIST);
    for record in records {
        record["list_id"] = json!(LIST);
        if record["id"] == "R_LIST_000" {
            record["thread_ts"] = json!(ROOT);
        }
    }
}
pub(crate) fn session() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T_LIST_FIXTURE".into(),
        user_id: "U_LIST_FIXTURE".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "fixture-not-a-token".into(),
    }
}
#[derive(Clone, Copy, Debug)]
pub(crate) enum Mode {
    Success,
    Empty,
    Malformed,
    WrongRoot,
    Offline,
    RateLimited,
    Held,
}
struct Held {
    sender: oneshot::Sender<Value>,
    reply: Value,
}
struct Ledger {
    document: Value,
    records: BTreeMap<String, Value>,
    requests: Vec<Value>,
    replies: u64,
    completed: u64,
    cancelled: u64,
    mode: Mode,
    held: Option<Held>,
    // Only the reaction-refresh case changes future server read responses.
    // A Held owns an earlier immutable response; POST receipts stay unchanged.
    reaction_readbacks: BTreeMap<String, Value>,
    incoming_readbacks: Vec<Value>,
    edit_readback: Option<Value>,
    compose: compose::Ledger,
}
#[derive(Clone)]
pub(crate) struct Model(Arc<Mutex<Ledger>>);
pub(crate) fn begin(s: &mut ShellState) -> Result<Model, String> {
    if !check_enabled() || !crate::slack_lists::fixture::enabled(s) {
        return Err("Closed comments fixture required".into());
    }
    let loaded = s
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.as_ref())
        .ok_or("Loaded List required")?;
    if loaded.document.id != LIST {
        return Err("Comments fixture seed missing".into());
    }
    let records = loaded
        .records
        .records
        .iter()
        .map(|r| (r.id.clone(), r.raw.clone()))
        .collect();
    let model = Model(Arc::new(Mutex::new(Ledger {
        document: loaded.document.raw.clone(),
        records,
        requests: vec![],
        replies: 0,
        completed: 0,
        cancelled: 0,
        mode: Mode::Success,
        held: None,
        reaction_readbacks: BTreeMap::new(),
        incoming_readbacks: Vec::new(),
        edit_readback: None,
        compose: Default::default(),
    })));
    s.slack_lists.comments.fixture = Some(model.clone());
    Ok(model)
}
pub(crate) fn mode(model: &Model, mode: Mode) -> Result<(), String> {
    let mut ledger = model.0.lock().map_err(|_| "Comments ledger poisoned")?;
    if ledger.held.is_some() {
        return Err("Original held reply must be released".into());
    }
    ledger.mode = mode;
    Ok(())
}
pub(crate) fn release(model: &Model) -> Result<Value, String> {
    let held = model
        .0
        .lock()
        .map_err(|_| "Comments ledger poisoned")?
        .held
        .take()
        .ok_or("No held reply")?;
    let cancelled = held.sender.is_closed();
    let delivered = held.sender.send(held.reply).is_ok();
    Ok(json!({"delivered":delivered,"cancelledBeforeReply":cancelled}))
}
fn refresh_reaction_fixture(s: &ShellState, model: &Model) -> bool {
    compose::enabled(s)
        && std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref()
            == Ok("reaction-refresh")
        && s.slack_lists
            .comments
            .fixture
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(&current.0, &model.0))
}
fn deletion_fixture(s: &ShellState, model: &Model) -> bool {
    compose::enabled(s)
        && std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref() == Ok("deletions")
        && s.slack_lists
            .comments
            .fixture
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(&current.0, &model.0))
}
/// Read-only proof of the intentionally stale next closed server reply. The
/// deletion case never changes server rows to make the native reducer pass.
pub(crate) fn deletion_server_response(s: &ShellState, model: &Model) -> Result<Value, String> {
    if !deletion_fixture(s, model) {
        return Err("Original closed deletions fixture required".into());
    }
    let ledger = model.0.lock().map_err(|_| "Comments ledger poisoned")?;
    let mut raw = reply(ROOT, false);
    compose::append_readback(&ledger.compose, &mut raw);
    Ok(raw)
}
fn incoming_fixture(s: &ShellState, model: &Model) -> bool {
    compose::enabled(s)
        && std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref() == Ok("incoming")
        && s.slack_lists
            .comments
            .fixture
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(&current.0, &model.0))
}
pub(crate) fn incoming_message(ordinal: u8) -> Value {
    let (ts, text, style) = match ordinal {
        1 => (
            "1781524804.000001",
            "Live comment one \u{4e2d}\u{1f642}",
            json!({"bold":true}),
        ),
        2 => (
            "1781524805.000001",
            "Live comment two \u{65e5}\u{1f98a}",
            json!({"italic":true}),
        ),
        _ => return Value::Null,
    };
    json!({"type":"message","channel":"CCOMMENT01","ts":ts,"thread_ts":ROOT,"user":"U1","text":text,
        "blocks":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":text,"style":style}]}]}],
        "opaque":{"source":"incoming","ordinal":ordinal}})
}
fn append_incoming_readback(rows: &[Value], raw: &mut Value) {
    if let Some(messages) = raw["messages"].as_array_mut() {
        messages.extend(rows.iter().cloned());
    }
}
pub(crate) fn incoming_server_response(s: &ShellState, model: &Model) -> Result<Value, String> {
    if !incoming_fixture(s, model) {
        return Err("Original closed incoming fixture required".into());
    }
    let ledger = model.0.lock().map_err(|_| "Comments ledger poisoned")?;
    let mut raw = reply(ROOT, false);
    append_incoming_readback(&ledger.incoming_readbacks, &mut raw);
    Ok(raw)
}
/// Publish original raw socket rows only to future fixture HTTP replies. The
/// intentionally stale second row must be excluded by the real visit tombstone.
pub(crate) fn publish_incoming_readback(s: &ShellState, model: &Model) -> Result<(), String> {
    if !incoming_fixture(s, model) {
        return Err("Original closed incoming fixture required".into());
    }
    let mut ledger = model.0.lock().map_err(|_| "Comments ledger poisoned")?;
    if ledger.held.is_some() || !ledger.incoming_readbacks.is_empty() {
        return Err(
            "Incoming readback must publish once after original held reply was released".into(),
        );
    }
    ledger.incoming_readbacks = vec![incoming_message(1), incoming_message(2)];
    Ok(())
}
fn edit_fixture(s: &ShellState, model: &Model) -> bool {
    compose::enabled(s)
        && matches!(
            std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").as_deref(),
            Ok("edits" | "edit-ack")
        )
        && s.slack_lists
            .comments
            .fixture
            .as_ref()
            .is_some_and(|m| Arc::ptr_eq(&m.0, &model.0))
}
/// An independently supplied future HTTP page. Never rewrites held requests,
/// accepted POST receipts, or the current Page.raw to make an event pass.
pub(crate) fn publish_edit_readback(
    s: &ShellState,
    model: &Model,
    rows: Vec<Value>,
) -> Result<Value, String> {
    if !edit_fixture(s, model) || rows.is_empty() || rows.len() > 2 {
        return Err("Closed bounded edit readback required".into());
    }
    let mut l = model.0.lock().map_err(|_| "Comments ledger poisoned")?;
    if l.edit_readback.is_some() || l.held.is_some() {
        return Err("Readback must follow held release, once".into());
    }
    let raw = edit_readback_response(rows)?;
    l.edit_readback = Some(raw.clone());
    Ok(raw)
}
fn edit_readback_response(rows: Vec<Value>) -> Result<Value, String> {
    if rows.is_empty() || rows.len() > 2 {
        return Err("Bounded edit readback required".into());
    }
    let mut raw = reply(ROOT, false);
    let messages = raw["messages"].as_array_mut().unwrap();
    let mut seen = std::collections::BTreeSet::new();
    for row in rows {
        let ts = row["ts"].as_str().ok_or("Readback timestamp missing")?;
        if !seen.insert(ts.to_owned())
            || !["1781524801.000001", "1781525001.000001"].contains(&ts)
            || row["thread_ts"] != ROOT
            || row["channel"] != "CCOMMENT01"
        {
            return Err("Foreign edit readback".into());
        }
        if let Some(existing) = messages.iter_mut().find(|m| m["ts"] == ts) {
            *existing = row;
        } else {
            messages.push(row);
        }
    }
    raw["has_more"] = json!(false);
    raw["response_metadata"] = json!({"next_cursor":""});
    Ok(raw)
}
pub(crate) fn held_response(s: &ShellState, model: &Model) -> Result<Value, String> {
    if !refresh_reaction_fixture(s, model)
        && !deletion_fixture(s, model)
        && !incoming_fixture(s, model)
        && !edit_fixture(s, model)
    {
        return Err("Original closed held-read fixture required".into());
    }
    model
        .0
        .lock()
        .map_err(|_| "Comments ledger poisoned")?
        .held
        .as_ref()
        .map(|held| held.reply.clone())
        .ok_or_else(|| "No held reply".into())
}
fn apply_reaction_readbacks(updates: &BTreeMap<String, Value>, raw: &mut Value) {
    if let Some(rows) = raw["messages"].as_array_mut() {
        for row in rows {
            if let Some(reactions) = row["ts"].as_str().and_then(|ts| updates.get(ts)) {
                row["reactions"] = reactions.clone();
            }
        }
    }
}
/// Update only later closed read responses, never the held body or POST receipt.
pub(crate) fn reaction_readback(
    s: &ShellState,
    model: &Model,
    ts: &str,
    user: &str,
    name: &str,
) -> Result<Value, String> {
    if !refresh_reaction_fixture(s, model) || user.is_empty() || name.is_empty() {
        return Err("Original closed reaction-refresh fixture required".into());
    }
    let mut ledger = model.0.lock().map_err(|_| "Comments ledger poisoned")?;
    let held_before = ledger.held.as_ref().ok_or("No held reply")?.reply.clone();
    let mut raw = reply(ROOT, false);
    compose::append_readback(&ledger.compose, &mut raw);
    apply_reaction_readbacks(&ledger.reaction_readbacks, &mut raw);
    let row = raw["messages"]
        .as_array()
        .and_then(|rows| {
            rows.iter().find(|row| {
                row["ts"].as_str() == Some(ts) && row["thread_ts"].as_str() == Some(ROOT)
            })
        })
        .ok_or("Known server reply required")?;
    let message = serde_json::from_value(row.clone()).map_err(|_| "Malformed server reply")?;
    let mut bag = super_platinum_core::state::ChannelMessages {
        messages: vec![message],
        ..Default::default()
    };
    if !bag.apply_reaction(ts, user, name, true) {
        return Err("Future server reaction was not new".into());
    }
    let reactions = json!(bag.messages[0].reactions);
    ledger
        .reaction_readbacks
        .insert(ts.into(), reactions.clone());
    let unchanged = ledger
        .held
        .as_ref()
        .is_some_and(|held| held.reply == held_before);
    if !unchanged {
        return Err("Held server response changed".into());
    }
    Ok(json!({"ts":ts,"reactions":reactions,"heldUnchanged":unchanged}))
}
fn fields(request: &PreparedRequest) -> Result<(String, BTreeMap<String, String>), Error> {
    let url = url::Url::parse(&request.url).map_err(|_| unavailable())?;
    if request.method != "POST" || url.scheme() != "https" || url.host_str() != Some("example.test")
    {
        return Err(unavailable());
    }
    let method = url
        .path()
        .strip_prefix("/api/")
        .ok_or_else(unavailable)?
        .to_owned();
    let RequestBody::Form(fields) = &request.body else {
        return Err(unavailable());
    };
    let mut args = BTreeMap::new();
    for (key, value) in fields {
        if key == "token" {
            if value != "fixture-not-a-token" {
                return Err(unavailable());
            }
            continue;
        }
        if args.insert(key.clone(), value.clone()).is_some() {
            return Err(unavailable());
        }
    }
    let expected: Vec<(&str, &str)> = match method.as_str() {
        "files.info" => vec![("file", LIST)],
        "lists.records.info" => vec![
            ("list_id", LIST),
            ("id", args.get("id").ok_or_else(unavailable)?),
            ("include_is_subscribed", "true"),
            ("_x_reason", "get-record-subtasks"),
        ],
        "experiments.getByUser" => vec![
            (
                "ssb_platform",
                args.get("ssb_platform").ok_or_else(unavailable)?,
            ),
            ("ssb_release_channel", "prod"),
        ],
        "conversations.replies" => {
            let mut e = vec![
                ("channel", "CCOMMENT01"),
                ("ts", args.get("ts").ok_or_else(unavailable)?),
                ("inclusive", "true"),
                ("limit", "28"),
                ("cached_latest_updates", "{}"),
                ("_x_reason", "history-api/fetchReplies"),
                ("include_tombstones", "true"),
            ];
            if args.contains_key("cursor") {
                e.push(("cursor", CURSOR));
            }
            e
        }
        _ => return Err(unavailable()),
    };
    if args.len() != expected.len()
        || expected
            .iter()
            .any(|(k, v)| args.get(*k).map(String::as_str) != Some(*v))
    {
        return Err(unavailable());
    }
    if method == "experiments.getByUser"
        && args["ssb_platform"] != std::env::consts::OS.replace("macos", "darwin")
    {
        return Err(unavailable());
    }
    Ok((method, args))
}
fn reply(ts: &str, next: bool) -> Value {
    let synthetic = ts == format!("{LIST}-R_LIST_001") || ts == SYNTHETIC_ROOT;
    let root = if synthetic { SYNTHETIC_ROOT } else { ROOT };
    let id = if synthetic {
        "R_LIST_001"
    } else {
        "R_LIST_000"
    };
    if next {
        return json!({"ok":true,"channel":"CCOMMENT01","messages":[{"ts":"1781524802.000001","thread_ts":root,"user":"U1","text":"Second page retained."}],"has_more":false});
    }
    let mut response = json!({"ok":true,"channel":"CCOMMENT01","messages":[
        {"ts":root,"subtype":"list_record_comment","slack_list":{"list_id":LIST,"list_record_id":id},"text":"Record comment root","unknown":{"keep":true}},
        {"ts":if synthetic{"1781524901.000001"}else{"1781524801.000001"},"thread_ts":root,"user":"U1","text":if synthetic{"Synthetic root resolved."}else{"First page comment."}}
    ],"has_more":!synthetic,"response_metadata":{"next_cursor":if synthetic{""}else{CURSOR}}});
    if !synthetic {
        response["messages"][1]["blocks"] = json!([{"type":"rich_text","elements":[
            {"type":"rich_text_section","elements":[{"type":"text","text":"First page comment.","style":{"bold":true}}]},
            {"type":"rich_text_section","elements":[{"type":"text","text":"Review ready \u{4e2d}\u{1f642}."}]}
        ]}]);
        response["messages"][1]["reactions"] =
            json!([{"name":"wave","count":2,"users":["U1","U2"]}]);
    }
    response
}
struct Pending {
    model: Model,
    done: bool,
}
impl Drop for Pending {
    fn drop(&mut self) {
        if let Ok(mut ledger) = self.model.0.lock() {
            ledger.completed += 1;
            if !self.done {
                ledger.cancelled += 1;
            }
        }
    }
}
pub(crate) async fn request(
    state: Signal<ShellState>,
    request: &PreparedRequest,
) -> Result<Value, Error> {
    let model = {
        let s = state.peek();
        if !enabled(&s) {
            return Err(unavailable());
        }
        s.slack_lists.comments.fixture.clone().unwrap()
    };
    let (method, args) = fields(request)?;
    let mut pending = Pending {
        model: model.clone(),
        done: false,
    };
    let (immediate, wait) = {
        let mut l = model.0.lock().map_err(|_| unavailable())?;
        let serial = l.requests.len() + 1;
        l.requests
            .push(json!({"serial":serial,"method":method,"args":args}));
        match method.as_str() {
            "files.info" => (Some(Ok(json!({"ok":true,"file":l.document}))), None),
            "lists.records.info" => {
                let row = l.records.get(&args["id"]).ok_or_else(unavailable)?;
                (
                    Some(Ok(
                        json!({"ok":true,"list_id":LIST,"record":row,"subtasks":[]}),
                    )),
                    None,
                )
            }
            "experiments.getByUser" => (
                Some(Ok(
                    json!({"ok":true,"assignments":{"lists_pilot":{"group":"on"},"lists_pilot_teams":{"group":"on"}}}),
                )),
                None,
            ),
            "conversations.replies" => {
                if ![ROOT, SYNTHETIC_ROOT, format!("{LIST}-R_LIST_001").as_str()]
                    .contains(&args["ts"].as_str())
                {
                    return Err(unavailable());
                }
                l.replies += 1;
                let mode = std::mem::replace(&mut l.mode, Mode::Success);
                let mut raw = reply(&args["ts"], args.contains_key("cursor"));
                compose::append_readback(&l.compose, &mut raw);
                apply_reaction_readbacks(&l.reaction_readbacks, &mut raw);
                append_incoming_readback(&l.incoming_readbacks, &mut raw);
                if args["ts"] == ROOT && !args.contains_key("cursor") {
                    if let Some(readback) = &l.edit_readback {
                        raw = readback.clone();
                    }
                }
                match mode {
                    Mode::Held => {
                        let (tx, rx) = oneshot::channel();
                        l.held = Some(Held {
                            sender: tx,
                            reply: raw,
                        });
                        (None, Some(rx))
                    }
                    Mode::Success => (Some(Ok(raw)), None),
                    Mode::Empty => (
                        Some(Ok(json!({"ok":true,"messages":[],"has_more":false}))),
                        None,
                    ),
                    Mode::Malformed => (Some(Ok(json!({"ok":true,"messages":false}))), None),
                    Mode::WrongRoot => (
                        Some(Ok(
                            json!({"ok":true,"messages":[{"ts":"1781524999.000001","thread_ts":"1781524998.000001","text":"Foreign reply"}]}),
                        )),
                        None,
                    ),
                    Mode::Offline => (
                        Some(Err(Error::Offline("Synthetic disconnected read".into()))),
                        None,
                    ),
                    Mode::RateLimited => (
                        Some(Err(Error::RateLimited {
                            retry_after_secs: Some(2),
                        })),
                        None,
                    ),
                }
            }
            _ => return Err(unavailable()),
        }
    };
    let result = if let Some(rx) = wait {
        rx.await.map_err(|_| unavailable())
    } else {
        immediate.unwrap()
    };
    pending.done = true;
    result
}
// Non-security evidence checksum over canonical sorted JSON. Exact touched rows
// remain in every snapshot; the original full map is emitted once for replay.
fn record_proof<'a>(records: impl Iterator<Item = (&'a String, &'a Value)>) -> (Value, Value) {
    let mut touched = BTreeMap::new();
    let mut unrelated = BTreeMap::new();
    for (id, raw) in records {
        if matches!(id.as_str(), "R_LIST_000" | "R_LIST_001") {
            touched.insert(id, raw);
        } else {
            unrelated.insert(id, raw);
        }
    }
    let bytes = serde_json::to_vec(&unrelated).expect("Synthetic records serialize");
    let hash = bytes.iter().fold(0xcbf29ce484222325_u64, |hash, b| {
        (hash ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    });
    (
        json!(touched),
        json!({"count":unrelated.len(),"byteLength":bytes.len(),"fnv1a64":format!("{hash:016x}")}),
    )
}
pub(crate) fn initial_records(model: &Model) -> Value {
    json!(model.0.lock().unwrap().records)
}
pub(crate) fn snapshot(s: &ShellState, model: &Model) -> Value {
    let l = model.0.lock().unwrap();
    let current = s.slack_lists.view.as_ref();
    let entry = s.slack_lists.comments.entry.as_ref();
    let (initial_records, initial_unrelated) = record_proof(l.records.iter());
    let proof = current
        .and_then(|v| v.loaded.as_ref())
        .map(|l| record_proof(l.records.records.iter().map(|r| (&r.id, &r.raw))));
    let comments = entry.map(|e| {
        let reactions = e.rows.iter().map(|m| {
            let counts = m.reactions.iter().map(|r| json!({"name":r.name,"count":r.count})).collect::<Vec<_>>();
            json!({"ts":m.ts,"reactions":counts})
        }).collect::<Vec<_>>();
        let mut value = json!({"active":e.owner.active(s),"record":e.target.record_id(),"root":e.target.root_ts(),"requestTs":e.target.request_ts(),"generation":e.owner.serial,
            "read":e.read,"deleted":e.deleted,"live":e.live,"liveTs":e.live.iter().filter_map(|m|m.ts.as_deref()).collect::<Vec<_>>(),
            "accepted":e.accepted,"acceptedTs":e.accepted.iter().filter_map(|m|m.ts.as_deref()).collect::<Vec<_>>(),"liveReactions":e.live_reactions,"reveal":e.reveal,
            "loading":e.loading,"error":e.error,"retryAt":e.retry_at,"retryAfter":e.retry_at.saturating_sub(now()).max(0),
            "page":e.page.as_ref().map(Page::raw),"hasMore":e.page.as_ref().is_some_and(Page::has_more),"visibleTs":e.rows.iter().map(|m|m.ts.as_str()).collect::<Vec<_>>(),
            "reactions":reactions});
        value["pageMessages"]=json!(e.page.as_ref().map(Page::messages));
        value["editCustody"]=json!(e.live_edits.keys().collect::<Vec<_>>());
        value
    });
    json!({"account":s.core.active_account,"epoch":s.core.account_epoch,"noSession":s.core.session.is_none(),"noTransport":s.core.transport.is_none(),
        "writes":crate::slack_lists::fixture::attempt_count(s),"networkCalls":0,
        "ledger":{"requests":l.requests,"replies":l.replies,"completed":l.completed,"cancelled":l.cancelled,"held":l.held.is_some()},
        "initialDocument":l.document,"initialRecords":initial_records,"initialUnrelated":initial_unrelated,
        "document":current.and_then(|v|v.loaded.as_ref()).map(|l|&l.document.raw),
        "records":proof.as_ref().map(|p|&p.0),"unrelated":proof.as_ref().map(|p|&p.1),
        "detail":current.and_then(record_detail::presentation).map(|d|json!({"record":d.record_id,"open":d.open_generation})),
        "comments":comments})
}
pub(crate) fn replace_record(s: &mut ShellState) -> Result<(), String> {
    if !enabled(s) {
        return Err("Closed comments replacement required".into());
    }
    let records = &mut s.slack_lists.fixture.as_mut().unwrap().records;
    let row = records["records"]
        .as_array_mut()
        .and_then(|rows| rows.iter_mut().find(|r| r["id"] == "R_LIST_001"))
        .ok_or("Replacement row missing")?;
    let field = row["fields"]
        .as_array_mut()
        .and_then(|f| f.iter_mut().find(|v| v["key"] == "estimate"))
        .ok_or("Replacement field missing")?;
    field["number"] = json!([19]);
    let team = s.core.active_team.clone().ok_or("Fixture team missing")?;
    let generation = s
        .core
        .workspaces
        .get(&team)
        .ok_or("Fixture workspace missing")?
        .rt_generation;
    let event = super_platinum_core::slack::events::RtEvent::Unknown(
        serde_json::from_value(json!({"type":"list_cells_updated","list_id":LIST,
        "cells":[{"row_id":"R_LIST_001","column_id":"C_NUMBER","key":"estimate","number":[19]}]}))
        .map_err(|_| "Fixture event shape")?,
    );
    if !crate::slack_lists::events::prepare(s, &team, generation, &event) {
        return Err("Original List event refused".into());
    }
    crate::slack_lists::fixture::refresh(s);
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use super_platinum_core::{
        slack::{api, client::SlackClient},
        slack_lists::{
            Document, Record,
            comments::{ReplyQuery, Tombstones},
        },
    };
    fn target(synthetic: bool) -> Target {
        let doc = Document::parse(
            json!({"id":LIST,"mode":"list","list_metadata":{"schema":[],"views":[]}}),
        )
        .unwrap();
        let mut row =
            json!({"id":if synthetic{"R_LIST_001"}else{"R_LIST_000"},"list_id":LIST,"fields":[]});
        if !synthetic {
            row["thread_ts"] = json!(ROOT);
        }
        Target::capture(&doc, &Record::parse(row).unwrap()).unwrap()
    }
    #[test]
    fn future_edit_readback_is_independent_and_decodes_one_canonical_tail() {
        let original = reply(ROOT, false);
        let held = original.clone();
        let post = json!({"type":"message","channel":"CCOMMENT01","ts":"1781525001.000001","thread_ts":ROOT,"user":"U0","client_msg_id":"original","text":"Original ACK","reactions":[{"name":"wave","count":1,"users":["U_RT_REACTOR"]}]});
        let mut canonical = post.clone();
        canonical["text"] = json!("Canonical sent reply.");
        canonical["reactions"] = json!([]);
        let future = edit_readback_response(vec![canonical.clone()]).unwrap();
        assert_eq!(held, original);
        assert_eq!(post["text"], "Original ACK");
        assert_eq!(post["reactions"][0]["count"], 1);
        assert_eq!(future["messages"][1], original["messages"][1]);
        assert_eq!(future["messages"][2], canonical);
        let page = Page::parse(&target(false), future).unwrap();
        assert_eq!(page.messages().len(), 3);
        assert!(!page.has_more());
        assert_eq!(page.cursor(), None);
    }
    #[test]
    fn edit_readback_rejects_foreign_root_channel_id_and_duplicate_rows() {
        let valid = json!({"channel":"CCOMMENT01","ts":"1781524801.000001","thread_ts":ROOT,"text":"Changed"});
        assert!(edit_readback_response(vec![valid.clone(), valid.clone()]).is_err());
        assert!(edit_readback_response(vec![]).is_err());
        for (field, value) in [
            ("channel", "COTHER"),
            ("ts", "1999999999.000001"),
            ("thread_ts", "1781524998.000001"),
        ] {
            let mut foreign = valid.clone();
            foreign[field] = json!(value);
            assert!(edit_readback_response(vec![foreign]).is_err());
        }
    }
    #[test]
    fn compact_records_hash_utf8_sorted_objects_and_keeps_exact_touched_rows() {
        let rows = BTreeMap::from([
            ("z".to_owned(), json!({"z":true,"a":"\u{4e2d}\u{1f642}\n"})),
            ("a".to_owned(), json!([2, 1])),
            ("R_LIST_000".to_owned(), json!({"opaque":true})),
        ]);
        let (touched, proof) = record_proof(rows.iter());
        assert_eq!(touched, json!({"R_LIST_000":{"opaque":true}}));
        assert_eq!(
            proof,
            json!({"count":2,"byteLength":42,"fnv1a64":"b80e91e571c0ce7d"})
        );
        assert_eq!(record_proof(rows.iter().rev()), (touched, proof));
    }
    #[test]
    fn exact_factories_and_reply_shapes_use_production_decoders() {
        let c = SlackClient::default();
        let s = session();
        for request in [
            api::file_info(&c, &s, LIST),
            api::list_record_subtasks(&c, &s, LIST, "R_LIST_000"),
            api::list_experiments(&c, &s),
        ] {
            assert!(fields(&request).is_ok());
        }
        for synthetic in [false, true] {
            let target = target(synthetic);
            let request = api::list_record_comments_replies(
                &c,
                &s,
                &target,
                &ReplyQuery::default(),
                Tombstones::Include,
            )
            .unwrap();
            assert!(fields(&request).is_ok());
            let page = Page::parse(&target, reply(&target.request_ts(), false)).unwrap();
            assert_eq!(
                page.target().root_ts(),
                Some(if synthetic { SYNTHETIC_ROOT } else { ROOT })
            );
            assert_eq!(
                Projection::observed(json!({"ok":true,"assignments":{}}))
                    .unwrap()
                    .visible(page.messages(), page.target().root_ts().unwrap())
                    .len(),
                1
            );
        }
    }
    #[test]
    fn closed_reader_rejects_other_endpoints_options_and_foreign_root() {
        let c = SlackClient::default();
        let s = session();
        for request in [
            api::file_delete(&c, &s, LIST),
            api::file_info(&c, &s, "FFOREIGN01"),
        ] {
            assert!(fields(&request).is_err());
        }
        let mut request = api::file_info(&c, &s, LIST);
        let RequestBody::Form(body) = &mut request.body else {
            panic!()
        };
        body.push(("include_shares".into(), "true".into()));
        assert!(fields(&request).is_err());
        assert!(Page::parse(&target(false), json!({"ok":true,"messages":false})).is_err());
        assert!(Page::parse(&target(false),json!({"ok":true,"messages":[{"ts":"1781524999.000001","thread_ts":"1781524998.000001"}]})).is_err());
    }
    #[test]
    fn future_reaction_overlay_keeps_held_body_and_post_receipt_unchanged() {
        let first = "1781524801.000001";
        let tail = "1781524803.000001";
        let post = json!({"ts":tail,"thread_ts":ROOT,"user":"U1","text":"Accepted reply",
            "client_msg_id":"original-uuid","blocks":[{"type":"rich_text","opaque":true}]});
        let mut old = reply(ROOT, false);
        old["messages"].as_array_mut().unwrap().push(post.clone());
        let held = old.clone();
        let updates = BTreeMap::from([
            (
                first.to_owned(),
                json!([{"name":"wave","count":3,"users":["U1","U2","U_RT_REACTOR"]}]),
            ),
            (
                tail.to_owned(),
                json!([{"name":"wave","count":1,"users":["U_RT_REACTOR"]}]),
            ),
        ]);
        let mut future = old.clone();
        apply_reaction_readbacks(&updates, &mut future);
        assert_eq!(held, old);
        assert_eq!(held["messages"][1]["reactions"][0]["count"], 2);
        assert!(held["messages"][2].get("reactions").is_none());
        assert!(post.get("reactions").is_none());
        assert_eq!(future["messages"][1]["reactions"][0]["count"], 3);
        assert_eq!(future["messages"][2]["reactions"][0]["count"], 1);
        let page = Page::parse(&target(false), future.clone()).unwrap();
        assert_eq!(page.cursor(), Some(CURSOR));
        assert!(page.has_more());
        future["messages"][1]["reactions"] = old["messages"][1]["reactions"].clone();
        future["messages"][2]
            .as_object_mut()
            .unwrap()
            .remove("reactions");
        assert_eq!(future, old);
    }
}
