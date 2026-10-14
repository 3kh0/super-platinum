//! Disconnected bulk-read backend; every wire receipt comes from a production builder.
use super::*;
use serde_json::{Value, json};
use super_platinum_core::{slack::client::RequestBody, state::CONFIRM_CLEAR_UNREADS};
pub(crate) const STAGES: &[&str] = &[
    "initial",
    "confirmation",
    "cancelled",
    "confirmation-checked",
    "sending",
    "cleared",
    "undo-sending",
    "undone",
    "expiry-ready",
    "expired",
    "owner-ready",
    "owner-changed",
    "uncertain",
    "refreshed",
];
#[derive(Default)]
pub(crate) struct Fixture {
    pub ledger: Vec<Value>,
    pub pending: Option<Work>,
    pub raw: BTreeMap<String, Value>,
    pub reconciliations: Vec<Value>,
    pub valid: bool,
    pub cleared: bool,
}
pub(crate) fn requested() -> bool {
    std::env::var("SUPER_PLATINUM_BULK_READ_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("bulk-read" | "bulk-read-narrow")
        )
}
pub(crate) fn directory() -> Result<(std::path::PathBuf, String), String> {
    if !requested() {
        return Err("Bulk read fixture opt-in missing".into());
    }
    let path = std::path::PathBuf::from(
        std::env::var_os("SUPER_PLATINUM_BULK_READ_DIR").ok_or("Private directory missing")?,
    );
    let nonce = std::env::var("SUPER_PLATINUM_BULK_READ_NONCE").map_err(|_| "Nonce missing")?;
    if !path.is_absolute()
        || !path
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.starts_with("bulk-read-"))
        || nonce.len() != 32
        || !nonce
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err("Invalid bulk-read fixture configuration".into());
    }
    let meta = std::fs::symlink_metadata(&path).map_err(|_| "Private directory missing")?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err("Invalid private directory".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return Err("Reparse directory refused".into());
        }
    }
    Ok((path, nonce))
}

pub(crate) fn enabled(s: &ShellState) -> bool {
    (requested() || cfg!(test))
        && s.bulk_read.fixture.is_some()
        && s.core.session.is_none()
        && s.core.transport.is_none()
}
pub(crate) fn session(s: &ShellState) -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        user_id: s.core.workspaces["T1"].self_user_id.clone(),
        enterprise_id: None,
        name: "Disconnected bulk-read fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-not-a-token".into(),
    }
}
pub(crate) fn install(s: &mut ShellState) {
    crate::channel_mute::fixture::install(s);
    s.core.active_account = Some("bulk-read-disconnected".into());
    s.overlay = None;
    s.app_modal = None;
    s.toast = None;
    let raw = BTreeMap::from([
        (CONFIRM_CLEAR_UNREADS.into(), json!(true)),
        ("unknown_pref".into(), json!({"keep":[1,2]})),
    ]);
    s.core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .clear_unreads_preference = ClearUnreadsPreference::from_prefs(&raw);
    s.bulk_read = State {
        fixture: Some(Fixture {
            raw,
            valid: true,
            ..Default::default()
        }),
        ..Default::default()
    };
    let c = Context::capture(s).unwrap();
    let baseline = reconcile::Baseline::capture(s, &c);
    let id = next_id();
    s.bulk_read
        .entries
        .entry(c.owner.clone())
        .or_default()
        .reconciling = Some(id);
    let raw = counts(s, false);
    let accepted = reconcile::apply_fixture(s, &c, &baseline, id, raw);
    finish_reconcile(s, &c, id, accepted);
    s.bulk_read.fixture.as_mut().unwrap().valid &= accepted;
}
fn receipt(request: &PreparedRequest) -> Value {
    let RequestBody::Form(fields) = &request.body else {
        panic!("Bulk fixture expected form")
    };
    let url = url::Url::parse(&request.url).unwrap();
    let fields: Vec<_> = fields
        .iter()
        .filter(|(k, _)| k != "token")
        .cloned()
        .collect();
    json!({"method":request.method,"url":request.url,"path":url.path(),"rawQuery":url.query(),"fields":fields,"retrySafe":request.retry_safe()})
}
fn record(s: &mut ShellState, request: &PreparedRequest, kind: &str, response: Value) -> usize {
    let f = s.bulk_read.fixture.as_mut().unwrap();
    let index = f.ledger.len();
    let mut row = receipt(request);
    row["sequence"] = json!(index);
    row["kind"] = json!(kind);
    row["response"] = response;
    f.ledger.push(row);
    index
}
fn fresh(s: &mut ShellState, c: &Context) -> ClearUnreadsPreference {
    let raw = json!({"ok":true,"prefs":s.bulk_read.fixture.as_ref().unwrap().raw});
    record(
        s,
        &api::clear_unreads_preference_request(&c.client, &c.session),
        "preference-read",
        raw.clone(),
    );
    api::parse_clear_unreads_preference(&raw).unwrap()
}
pub(crate) fn read(mut state: Signal<ShellState>, r: PrefRead) -> bool {
    if !enabled(&state.peek()) {
        return false;
    }
    let next = {
        let mut s = state.write();
        let p = fresh(&mut s, &r.context);
        preference::finish_read(&mut s, &r, Ok(p))
    };
    if let Some(next) = next {
        dispatch(state, next);
    }
    true
}
pub(crate) fn preference_write(mut state: Signal<ShellState>, w: PrefWork) -> bool {
    if !enabled(&state.peek()) {
        return false;
    }
    let mut s = state.write();
    let p = fresh(&mut s, &w.context);
    let Some(request) = preference::preflight(&mut s, &w, &p) else {
        s.bulk_read.fixture.as_mut().unwrap().valid = false;
        return true;
    };
    record(&mut s, &request, "preference-write", json!({"ok":true}));
    s.bulk_read
        .fixture
        .as_mut()
        .unwrap()
        .raw
        .insert(CONFIRM_CLEAR_UNREADS.into(), json!(false));
    if preference::finish_write(&mut s, &w, Ok(())) {
        let p = fresh(&mut s, &w.context);
        preference::finish_check(&mut s, &w, Ok(p));
    }
    true
}
pub(crate) fn write(mut state: Signal<ShellState>, w: Work) -> Option<Work> {
    if !enabled(&state.peek()) {
        return Some(w);
    }
    let mut s = state.write();
    if !sending(&s, &w) {
        cancel_unsent(&mut s, &w);
        return None;
    }
    record(
        &mut s,
        &w.request,
        if w.kind == Kind::Clear {
            "clear"
        } else {
            "undo"
        },
        Value::Null,
    );
    let f = s.bulk_read.fixture.as_mut().unwrap();
    f.valid &= f.pending.replace(w).is_none();
    None
}
fn counts(s: &ShellState, cleared: bool) -> Value {
    let channels:Vec<_>=s.core.workspaces["T1"].channels.values().map(|c|json!({"id":c.id,"has_unreads":!cleared&&c.id=="C2","unread_count":if !cleared&&c.id=="C2"{3}else{0},"unread_count_display":if !cleared&&c.id=="C2"{3}else{0},"mention_count":if !cleared&&c.id=="C2"{1}else{0},"last_read":if cleared{"1719800600.000000"}else{"1719800000.000000"}})).collect();
    json!({"ok":true,"channels":channels,"ims":[],"groups":[],"mpims":[],"activity_v2":{"at_user":if cleared{0}else{2}}})
}
pub(crate) fn snapshot(s: &ShellState) -> Value {
    let w = &s.core.workspaces["T1"];
    json!({"channels":w.channels,"activityUnread":w.activity_unread_count,"threadCounts":w.thread_counts.snapshot,"dockCounts":w.dock_counts.channel_badges()})
}
fn reconcile(s: &mut ShellState, c: &Context, b: &reconcile::Baseline, id: u64) -> bool {
    let before = snapshot(s);
    let raw = counts(s, s.bulk_read.fixture.as_ref().unwrap().cleared);
    let sequence = record(
        s,
        &api::client_counts(&c.client, &c.session),
        "counts",
        raw.clone(),
    );
    let accepted = reconcile::apply_fixture(s, c, b, id, raw.clone());
    finish_reconcile(s, c, id, accepted);
    let after = snapshot(s);
    let f = s.bulk_read.fixture.as_mut().unwrap();
    f.valid &= accepted;
    f.reconciliations.push(json!({"sequence":sequence,"accepted":accepted,"scope":"counts-only","before":before,"after":after,"response":raw}));
    accepted
}
pub(crate) fn refresh(
    mut state: Signal<ShellState>,
    c: Arc<Context>,
    b: Arc<reconcile::Baseline>,
    id: u64,
) -> bool {
    if !enabled(&state.peek()) {
        return false;
    }
    reconcile(&mut state.write(), &c, &b, id);
    true
}
pub(crate) fn complete(s: &mut ShellState, unknown: bool) -> bool {
    let w = s
        .bulk_read
        .fixture
        .as_mut()
        .unwrap()
        .pending
        .take()
        .unwrap();
    let before = snapshot(s);
    let response = if unknown {
        Value::Null
    } else if w.kind == Kind::Clear {
        json!({"ok":true,"undo_ts":"1719800700.000001"})
    } else {
        json!({"ok":true})
    };
    let f = s.bulk_read.fixture.as_mut().unwrap();
    let row = f
        .ledger
        .iter_mut()
        .rev()
        .find(|r| r["kind"] == "clear" || r["kind"] == "undo")
        .unwrap();
    row["response"] = response.clone();
    row["outcome"] = json!(if unknown { "unknown" } else { "ack" });
    if !unknown {
        f.cleared = w.kind == Kind::Clear;
    }
    let result = if unknown {
        Err(Error::Transport("Synthetic lost response".into()))
    } else {
        Ok(response)
    };
    let id = finish_write(s, &w, result);
    let unchanged = before == snapshot(s);
    if let Some(id) = id {
        reconcile(s, &w.context, &w.baseline, id);
    }
    unchanged
}
