//! Exact named synthetic HTTP boundary. Same builders, positive decoder and reducers.
use super::*;
pub(crate) mod field_delete;
mod filter;
pub(crate) mod grouping;
mod text_create;
pub(crate) mod view_order;
pub(crate) use text_create::select as select_create;
pub(crate) use text_create::{
    EntityCase, FieldCreateMode, field_create_check_enabled, field_create_mode, seed_text_create,
    seed_text_create_reopen, stale_text_create_file, text_create_custody, text_create_registry,
};
mod sort;
pub(crate) use filter::{
    compound_filter_check_enabled, filter_check_enabled, filter_registry_empty,
    number_filter_check_enabled, seed_filter, seed_filter_reopen, stale_filter_file,
};
#[cfg(test)]
pub(crate) use sort::discard_sort_fixture;
mod visibility;
pub(crate) use sort::{
    ordered_sort_check_enabled, seed_sort, seed_sort_reopen, sort_check_enabled,
    sort_registry_empty, stale_sort_file,
};
use std::sync::{Mutex, OnceLock};
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{
        client::{PreparedRequest, RequestBody},
        transport::ListColumnRenameFailure,
    },
};
pub(crate) use visibility::{
    seed_visibility, seed_visibility_reopen, stale_visibility_file, visibility_check_enabled,
};
#[derive(Default)]
struct Ledger {
    requests: Vec<Value>,
    responses: Vec<Value>,
    pending: Option<tokio::sync::oneshot::Sender<()>>,
    held_read: Option<tokio::sync::oneshot::Sender<()>>,
    hold_next_read: bool,
    precision_file_once: Option<Value>,
    finished: Vec<Value>,
    server: Option<Value>,
    select_options: Option<(u64, Value)>,
}
fn ledger() -> &'static Mutex<Ledger> {
    static LEDGER: OnceLock<Mutex<Ledger>> = OnceLock::new();
    LEDGER.get_or_init(Default::default)
}
pub(crate) fn check_enabled() -> bool {
    std::env::var("SUPER_PLATINUM_LIST_RENAME_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("slack-list-column-rename" | "slack-list-column-rename-narrow")
        )
}
pub(crate) fn view_check_enabled() -> bool {
    std::env::var("SUPER_PLATINUM_LIST_VIEW_RENAME_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("slack-list-view-rename" | "slack-list-view-rename-narrow")
        )
}
pub(crate) fn create_check_enabled() -> bool {
    std::env::var("SUPER_PLATINUM_LIST_VIEW_CREATE_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("slack-list-view-create" | "slack-list-view-create-narrow")
        )
}
pub(crate) fn precision_check_enabled() -> bool {
    precision_gate(
        std::env::var("SUPER_PLATINUM_LIST_PRECISION_CHECK")
            .ok()
            .as_deref(),
        std::env::var("SUPER_PLATINUM_FIXTURE").ok().as_deref(),
    )
}
fn precision_gate(flag: Option<&str>, alias: Option<&str>) -> bool {
    flag == Some("1")
        && matches!(
            alias,
            Some("slack-list-number-precision" | "slack-list-number-precision-narrow")
        )
}
pub(crate) fn number_rename_check_enabled() -> bool {
    number_rename_gate(
        std::env::var("SUPER_PLATINUM_LIST_NUMBER_RENAME_CHECK")
            .ok()
            .as_deref(),
        std::env::var("SUPER_PLATINUM_FIXTURE").ok().as_deref(),
    )
}
fn number_rename_gate(flag: Option<&str>, alias: Option<&str>) -> bool {
    flag == Some("1")
        && matches!(
            alias,
            Some("slack-list-number-rename" | "slack-list-number-rename-narrow")
        )
}
pub(crate) fn seed_number_rename(shell: &ShellState, server: Value) {
    assert!(number_rename_check_enabled() && enabled(shell));
    *ledger().lock().unwrap() = Ledger {
        server: Some(server),
        ..Default::default()
    };
}
pub(crate) fn stale_number_rename_file(shell: &ShellState) -> Result<(), String> {
    if !number_rename_check_enabled() || !enabled(shell) {
        return Err("Exact disconnected Number name checker required".into());
    }
    let e = entry(shell)
        .filter(|e| {
            e.stage == Stage::Uncertain
                && e.target == Target::Column("C_NUMBER".into())
                && matches!(e.intent, Some(Intent::Column(_)))
        })
        .ok_or("No uncertain Number name intent")?;
    ledger()
        .lock()
        .unwrap()
        .queue_precision_file(e.original.raw)
}
pub(crate) fn seed_precision(shell: &ShellState, server: Value) {
    assert!(precision_check_enabled() && enabled(shell));
    *ledger().lock().unwrap() = Ledger {
        server: Some(server),
        ..Default::default()
    };
}
pub(super) fn enabled(shell: &ShellState) -> bool {
    (check_enabled()
        || view_check_enabled()
        || create_check_enabled()
        || precision_check_enabled()
        || number_rename_check_enabled()
        || visibility_check_enabled()
        || sort_check_enabled()
        || filter_check_enabled()
        || grouping::check_enabled()
        || field_create_check_enabled()
        || field_delete::check_enabled()
        || view_order::check_enabled())
        && super::super::fixture::enabled(shell)
        && shell.core.session.is_none()
        && shell.core.transport.is_none()
}
pub(crate) fn install(shell: &mut ShellState, name: &str) {
    let server = if name.starts_with("slack-list-view-create") {
        super::super::fixture::view_create::install(shell, name)
    } else if name.starts_with("slack-list-view-rename") {
        super::super::fixture::view_rename::install(shell, name)
    } else {
        super::super::fixture::column_rename::install(shell, name)
    };
    // Only this exact disconnected seed initializes the private synthetic server.
    *ledger().lock().unwrap() = Ledger {
        server: Some(server),
        ..Default::default()
    };
}
pub(super) fn session() -> WorkspaceSession {
    WorkspaceSession {
        team_id: "T1".into(),
        user_id: "U1".into(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "fixture-not-a-token".into(),
    }
}
fn record(request: PreparedRequest) -> Result<String, Error> {
    let url = url::Url::parse(&request.url).map_err(|_| Error::Api("fixture_url".into()))?;
    if request.method != "POST" || url.host_str() != Some("example.test") {
        return Err(Error::Api("fixture_request_origin".into()));
    }
    let RequestBody::Form(fields) = request.body else {
        return Err(Error::Api("fixture_form".into()));
    };
    if fields.first() != Some(&("token".into(), "fixture-not-a-token".into())) {
        return Err(Error::Api("fixture_token".into()));
    }
    let path = url.path().to_owned();
    ledger()
        .lock()
        .unwrap()
        .requests
        .push(json!({"method":"POST","path":path,"fields":fields}));
    Ok(path)
}
pub(super) async fn read(
    state: Signal<ShellState>,
    guard: &Guard,
    stage: Stage,
    request: PreparedRequest,
) -> Result<Value, Error> {
    if !enabled(&state.peek()) || !guard.current(&state.peek(), stage) {
        return Err(Error::Api("fixture_retired".into()));
    }
    if view_order::check_enabled() {
        view_order::read_form(&request)?;
    }
    let delete_policy = if field_delete::check_enabled() {
        field_delete::policy_response(&request)?
    } else {
        None
    };
    let path = record(request)?;
    let (raw, held) = {
        let mut l = ledger().lock().unwrap();
        let raw = match path.as_str() {
            "/api/files.info" => {
                json!({"ok":true,"file":l.file_response(stage, precision_check_enabled() || number_rename_check_enabled() || visibility_check_enabled() || sort_check_enabled() || filter_check_enabled()
        || grouping::check_enabled() || field_create_check_enabled() || field_delete::check_enabled()
        || view_order::check_enabled())})
            }
            "/api/experiments.getByUser" => delete_policy
                .ok_or_else(|| Error::Api("fixture_delete_policy_not_configured".into()))?,
            "/api/team.prefs.get" => json!({"ok":true,"prefs":{"allow_lists":"full"}}),
            "/api/users.prefs.get" => {
                json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}})
            }
            _ => return Err(Error::Api("fixture_unknown_read".into())),
        };
        l.responses.push(raw.clone());
        let held = if stage == Stage::Checking && path == "/api/files.info" && l.hold_next_read {
            l.hold_next_read = false;
            let (tx, rx) = tokio::sync::oneshot::channel();
            l.held_read = Some(tx);
            Some(rx)
        } else {
            None
        };
        (raw, held)
    };
    if let Some(rx) = held {
        tokio::time::timeout(std::time::Duration::from_secs(30), rx)
            .await
            .map_err(|_| Error::Transport("fixture_timeout".into()))?
            .map_err(|_| Error::Transport("fixture_closed".into()))?;
    }
    // Exercise the same strict public JSON decoder before the positive file/policy parser.
    super_platinum_core::slash_commands::codec::decode(
        &serde_json::to_vec(&raw).unwrap(),
        1024 * 1024,
    )
    .map_err(|_| Error::Api("fixture_decode".into()))
}
pub(super) async fn write(
    state: Signal<ShellState>,
    guard: &Guard,
    permit: Permit,
) -> Result<Value, ListColumnRenameFailure> {
    if !enabled(&state.peek())
        || !guard.current(&state.peek(), Stage::Saving)
        || registry::blocked(&guard.key.owner)
    {
        return Err(ListColumnRenameFailure::NotRequested(
            Error::TransportNotConfigured,
        ));
    }
    if permit.is_reorder() {
        return view_order::write(state, guard, permit).await;
    }
    if permit.is_deletion() {
        return field_delete::write(state, guard, permit).await;
    }
    // Exact owned Grouping adapter; static fixtures cannot dispatch.
    if permit.is_grouping() {
        return grouping::write(state, guard, permit).await;
    }
    if field_create_check_enabled() {
        return text_create::write(state, guard, permit).await;
    }
    if filter_check_enabled() {
        return filter::write(state, guard, permit).await;
    }
    if sort_check_enabled() {
        return sort::write(state, guard, permit).await;
    }
    if visibility_check_enabled() {
        return visibility::write(state, guard, permit).await;
    }
    let request = permit.request(&state.peek().core.client, &session());
    let desired = with_registry(|r| r.entries[&guard.key].intent.as_ref().unwrap().input());
    let name = desired.name().unwrap_or_default();
    let target = with_registry(|r| r.entries[&guard.key].target.clone());
    record(request).map_err(ListColumnRenameFailure::NotRequested)?;
    let (tx, rx) = tokio::sync::oneshot::channel();
    {
        let mut l = ledger().lock().unwrap();
        let positive = target.creation().is_some()
            && std::env::var("SUPER_PLATINUM_FIXTURE").as_deref() == Ok("slack-list-view-create");
        if positive {
            let mut server = l.server.clone().unwrap();
            super::super::fixture::view_create::add(
                &mut server,
                name,
                target.creation().unwrap(),
                target.grouping(),
            );
            // Deliberately a mutation response.list, never files.info.file.
            l.responses.push(json!({"ok":true,"list":{"id":server["id"],"list_metadata":server["list_metadata"]},"opaque_reply":{"retain":true}}));
        } else {
            l.responses
                .push(json!({"syntheticFailure":"response_lost"}));
        }
        if l.pending.is_some() {
            return Err(ListColumnRenameFailure::Unconfirmed(Error::Transport(
                "fixture_duplicate_write".into(),
            )));
        }
        l.pending = Some(tx);
    }
    tokio::time::timeout(std::time::Duration::from_secs(30), rx)
        .await
        .map_err(|_| {
            ListColumnRenameFailure::Unconfirmed(Error::Transport("fixture_timeout".into()))
        })?
        .map_err(|_| {
            ListColumnRenameFailure::Unconfirmed(Error::Transport("fixture_closed".into()))
        })?;
    {
        let mut l = ledger().lock().unwrap();
        if let Some(layout) = target.creation() {
            super::super::fixture::view_create::add(
                l.server.as_mut().unwrap(),
                name,
                layout,
                target.grouping(),
            );
            if std::env::var("SUPER_PLATINUM_FIXTURE").as_deref() == Ok("slack-list-view-create") {
                return Ok(l.responses.last().unwrap().clone());
            }
        } else if let Input::Precision(value) = &desired {
            let column = l.server.as_mut().unwrap()["list_metadata"]["schema"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|c| c["id"].as_str() == target.id())
                .unwrap();
            column["options"]["precision"] = json!(value);
        } else {
            let schema = l.server.as_mut().unwrap()["list_metadata"]
                [if target.is_view() { "views" } else { "schema" }]
            .as_array_mut()
            .unwrap();
            schema
                .iter_mut()
                .find(|c| c["id"].as_str() == target.id())
                .unwrap()["name"] = json!(name);
        }
    }
    Err(ListColumnRenameFailure::Unconfirmed(Error::Transport(
        "fixture_response_lost".into(),
    )))
}
pub(super) fn finished(guard: &Guard, kind: &str, applied: bool) {
    if check_enabled()
        || view_check_enabled()
        || create_check_enabled()
        || precision_check_enabled()
        || number_rename_check_enabled()
        || visibility_check_enabled()
        || sort_check_enabled()
        || filter_check_enabled()
        || grouping::check_enabled()
        || field_create_check_enabled()
        || field_delete::check_enabled()
        || view_order::check_enabled()
    {
        ledger()
            .lock()
            .unwrap()
            .finished
            .push(json!({"serial":guard.serial,"kind":kind,"applied":applied}));
    }
}
pub(crate) fn release(shell: &ShellState, read: bool) -> Result<(), String> {
    if !enabled(shell) {
        return Err("Disconnected rename fixture required".into());
    }
    let tx = {
        let mut l = ledger().lock().unwrap();
        if read {
            l.held_read.take()
        } else {
            l.pending.take()
        }
    }
    .ok_or("Missing held response")?;
    tx.send(()).map_err(|_| "Held response closed".into())
}
pub(crate) fn hold_read(shell: &ShellState) -> Result<(), String> {
    if !enabled(shell) {
        return Err("Disconnected rename fixture required".into());
    }
    ledger().lock().unwrap().hold_next_read = true;
    Ok(())
}
pub(crate) fn snapshot(shell: &ShellState) -> Result<Value, String> {
    if !enabled(shell) {
        return Err("Disconnected rename fixture required".into());
    }
    let l = ledger().lock().unwrap();
    Ok(
        json!({"requests":l.requests,"responses":l.responses,"finished":l.finished,"server":l.server,"writePending":l.pending.is_some(),"readPending":l.held_read.is_some()}),
    )
}
pub(crate) fn reopen(shell: &mut ShellState) -> Result<u64, String> {
    if !enabled(shell) {
        return Err("Disconnected rename fixture required".into());
    }
    let server = ledger().lock().unwrap().server.clone().unwrap();
    // Opening is a new actual List visit. Canonical rows still come from the
    // untouched fixture population, never from the held metadata response.
    super::super::open_shell(
        shell,
        serde_json::from_value(server).map_err(|_| "fixture_file")?,
    );
    super::super::fixture::refresh(shell);
    generation(shell).ok_or("No reopened List".into())
}

/// Passive fixture inspection of the production retained receipt, not a synthetic acknowledgement.
pub(crate) fn retained_reply(shell: &ShellState) -> Option<Value> {
    if !enabled(shell) {
        return None;
    }
    entry(shell)?.raw_reply
}

/// Exact disconnected source mutation for a schema-race preflight. No live reads/writes.
pub(crate) fn grouping_schema(shell: &ShellState, key: &str, changed: bool) -> Result<(), String> {
    if !create_check_enabled() || !enabled(shell) {
        return Err("Disconnected new-view fixture required".into());
    }
    let original = shell
        .slack_lists
        .view
        .as_ref()
        .and_then(|v| v.loaded.as_ref())
        .and_then(|l| l.document.columns.iter().find(|c| c.key == key))
        .ok_or("Missing fixture grouping column")?
        .raw
        .clone();
    let mut l = ledger().lock().unwrap();
    if l.pending.is_some() || l.held_read.is_some() {
        return Err("Pending fixture response".into());
    }
    let column = l.server.as_mut().ok_or("Missing fixture server")?["list_metadata"]["schema"]
        .as_array_mut()
        .ok_or("Missing fixture schema")?
        .iter_mut()
        .find(|c| c["key"] == key)
        .ok_or("Missing fixture server grouping column")?;
    *column = original;
    if changed {
        column["type"] = json!("text");
    }
    Ok(())
}

// One response override only, never a canonical document mutation or future freshness policy.
impl Ledger {
    fn queue_precision_file(&mut self, original: Value) -> Result<(), String> {
        if self.server.is_none()
            || self.pending.is_some()
            || self.held_read.is_some()
            || self.hold_next_read
            || self.precision_file_once.is_some()
        {
            return Err("Precision response already pending or unseeded".into());
        }
        self.precision_file_once = Some(original);
        Ok(())
    }
    fn file_response(&mut self, stage: Stage, precision: bool) -> Value {
        if precision && stage == Stage::Checking {
            if let Some(value) = self.precision_file_once.take() {
                return value;
            }
        }
        self.server.clone().unwrap()
    }
}
pub(crate) fn stale_precision_file(shell: &ShellState) -> Result<(), String> {
    if !precision_check_enabled() || !enabled(shell) {
        return Err("Exact disconnected precision checker required".into());
    }
    let e = entry(shell)
        .filter(|e| e.stage == Stage::Uncertain && e.target.is_precision() && e.intent.is_some())
        .ok_or("No uncertain precision intent")?;
    ledger()
        .lock()
        .unwrap()
        .queue_precision_file(e.original.raw)
}

#[cfg(test)]
mod precision_tests {
    use super::*;
    #[test]
    fn exact_precision_flag_and_alias_are_both_required() {
        for alias in [
            "slack-list-number-precision",
            "slack-list-number-precision-narrow",
        ] {
            assert!(precision_gate(Some("1"), Some(alias)));
            for flag in [None, Some(""), Some("true"), Some("0")] {
                assert!(!precision_gate(flag, Some(alias)));
            }
        }
        for alias in [
            None,
            Some("slack-list"),
            Some("slack-list-number-precision-extra"),
        ] {
            assert!(!precision_gate(Some("1"), alias));
        }
    }
    #[test]
    fn stale_precision_snapshot_is_single_check_only_and_does_not_change_server() {
        let mut l = Ledger {
            server: Some(json!({"precision":2})),
            ..Default::default()
        };
        l.queue_precision_file(json!({"precision":0})).unwrap();
        assert!(l.queue_precision_file(json!({"precision":1})).is_err());
        assert_eq!(
            l.file_response(Stage::Preparing, true),
            json!({"precision":2})
        );
        assert_eq!(
            l.file_response(Stage::Checking, false),
            json!({"precision":2})
        );
        assert_eq!(
            l.file_response(Stage::Checking, true),
            json!({"precision":0})
        );
        assert_eq!(
            l.file_response(Stage::Checking, true),
            json!({"precision":2})
        );
        assert_eq!(l.server, Some(json!({"precision":2})));
    }
    #[test]
    fn stale_precision_override_refuses_unseeded_and_every_held_response() {
        assert!(Ledger::default().queue_precision_file(json!({})).is_err());
        for kind in 0..3 {
            let mut l = Ledger {
                server: Some(json!({})),
                ..Default::default()
            };
            let (tx, _rx) = tokio::sync::oneshot::channel();
            match kind {
                0 => l.pending = Some(tx),
                1 => l.held_read = Some(tx),
                _ => l.hold_next_read = true,
            }
            assert!(l.queue_precision_file(json!({})).is_err());
            assert!(l.precision_file_once.is_none());
        }
    }
}

#[cfg(test)]
mod number_rename_tests {
    use super::*;
    #[test]
    fn number_name_gate_never_borrows_precision_or_generic_rename_opt_in() {
        for alias in [
            "slack-list-number-rename",
            "slack-list-number-rename-narrow",
        ] {
            assert!(number_rename_gate(Some("1"), Some(alias)));
            for flag in [None, Some("0"), Some("true"), Some("")] {
                assert!(!number_rename_gate(flag, Some(alias)));
            }
        }
        for alias in [
            None,
            Some("slack-list-number-precision"),
            Some("slack-list-column-rename"),
            Some("slack-list"),
            Some("slack-list-number-rename-extra"),
        ] {
            assert!(!number_rename_gate(Some("1"), alias));
        }
    }
}

#[cfg(test)]
pub(crate) use text_create::close_date_fixture;

#[cfg(test)]
pub(crate) use text_create::close_channel_fixture;
#[cfg(test)]
pub(crate) use text_create::close_link_fixture;
#[cfg(test)]
pub(crate) use text_create::close_people_fixture;
