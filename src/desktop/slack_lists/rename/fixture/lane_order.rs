//! Exact disconnected Board lane move adapter over the shared ledger and production worker.
use super::*;
use super_platinum_core::slack_lists::BoardLanePlacement;
fn target(t: &Target) -> bool {
    matches!(t,Target::LaneOrder(a) if a.view=="V_LANE" && a.source=="OPT_DONE" && a.target=="OPT_PROGRESS" && a.placement==BoardLanePlacement::Before)
}
fn expected_grouping() -> Value {
    json!({"group_by":"status","group_by_column_id":"C_SELECT","opaque":{"retained":[1,null,true]},"order":[{"select":[]},{"select":["OPT_DONE"]},{"select":["OPT_PROGRESS"]}]})
}

pub(crate) fn check_enabled() -> bool {
    gate(
        std::env::var("SUPER_PLATINUM_LIST_BOARD_LANE_ORDER_CHECK")
            .ok()
            .as_deref(),
        std::env::var("SUPER_PLATINUM_FIXTURE").ok().as_deref(),
    )
}
fn gate(flag: Option<&str>, alias: Option<&str>) -> bool {
    flag == Some("1")
        && matches!(
            alias,
            Some("slack-list-board-lane-order" | "slack-list-board-lane-order-narrow")
        )
}
fn require(shell: &ShellState) -> Result<(), String> {
    if check_enabled() && enabled(shell) {
        Ok(())
    } else {
        Err("Exact disconnected Board lane ordering checker required".into())
    }
}
pub(crate) fn seed(shell: &ShellState, server: Value) {
    require(shell).unwrap();
    *ledger().lock().unwrap() = Ledger {
        server: Some(server),
        ..Default::default()
    };
}
/// Passive evidence of explicitly controlled initial offline setup, not a read ACK.
pub(crate) fn initial_readiness(shell: &ShellState) -> Result<Value, String> {
    require(shell)?;
    let view = shell
        .slack_lists
        .view
        .as_ref()
        .ok_or("Missing initial view")?;
    let loaded = view
        .loaded
        .as_ref()
        .ok_or("Missing initial fixture Loaded")?;
    if view.loading
        || view.stale
        || view.unavailable
        || selected_view(shell) != Some("V_LANE")
        || !registry_empty(shell)?
        || loaded.records.records.len() != 73
        || loaded.records.raw["response_metadata"]["next_cursor"] != "precision-opaque+/="
    {
        return Err("Initial ordering fixture is not ready".into());
    }
    Ok(
        json!({"provenance":"controlled-offline-seed","loading":view.loading,"stale":view.stale,"unavailable":view.unavailable,
        "selected":"V_LANE","document":loaded.document.raw,"population":loaded.records.raw}),
    )
}
pub(crate) fn registry_empty(shell: &ShellState) -> Result<bool, String> {
    require(shell)?;
    Ok(entry(shell).is_none() && super::super::presentation(shell).is_none())
}
pub(crate) fn custody(shell: &ShellState) -> Result<Value, String> {
    require(shell)?;
    let e = entry(shell)
        .filter(|e| target(&e.target))
        .ok_or("Missing lane move custody")?;
    Ok(
        json!({"account":shell.core.active_account,"epoch":shell.core.account_epoch,"serial":e.serial,
        "target":e.target.id(),"direction":"left","original":e.original.raw,"population":e.original_records.as_ref().map(|r| &r.raw),
        "intent":e.intent.as_ref().map(|i|match i {Intent::LaneOrder(i)=>json!({"original":i.original().raw,"view":i.view_id(),"direction":"left","grouping":i.grouping(),"order":i.desired_order()}),_=>Value::Null})}),
    )
}
pub(crate) fn refuse_old_confirmation(
    shell: &mut ShellState,
    old_serial: u64,
) -> Result<Value, String> {
    require(shell)?;
    let e = entry(shell)
        .filter(|e| e.stage == Stage::Editing && target(&e.target) && e.serial != old_serial)
        .ok_or("No replaced lane move choice")?;
    let before = custody(shell)?;
    let prepared = super::super::prepare(shell, old_serial).is_some();
    let closed = super::super::close_entry(shell, old_serial);
    if prepared
        || closed
        || custody(shell)? != before
        || entry(shell)
            .is_none_or(|a| a.serial != e.serial || a.job.is_some() || a.intent.is_some())
    {
        return Err("Old lane move callback changed authority".into());
    }
    Ok(
        json!({"oldSerial":old_serial,"currentSerial":e.serial,"confirm":prepared,"cancel":closed,"custody":before}),
    )
}
/// Pure stale callback admission proof, separate from the subsequent DOM move.
pub(crate) fn refuse_stale_action(shell: &mut ShellState) -> Result<Value, String> {
    require(shell)?;
    if !registry_empty(shell)? || snapshot(shell)?["requests"] != json!([]) {
        return Err("Stale probe must precede dispatch".into());
    }
    let action = super::super::lane_order::adjacent(
        shell,
        "OPT_DONE",
        super::super::lane_order::Direction::Left,
    )
    .ok_or("Missing live lane action")?;
    let before = snapshot(shell)?;
    let find = shell.slack_lists.view.as_ref().unwrap().layout.find.clone();
    shell
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .layout
        .find
        .revision = find.revision.checked_add(1).ok_or("Find exhausted")?;
    let opened = super::super::lane_order::open_shell(shell, action);
    shell.slack_lists.view.as_mut().unwrap().layout.find = find;
    if opened || !registry_empty(shell)? || snapshot(shell)? != before {
        return Err("Stale lane action admitted".into());
    }
    Ok(
        json!({"kind":"stale-find-revision-admission","opened":false,"registryEmpty":true,"requests":0,"restoredFind":true}),
    )
}
pub(crate) fn conflict_file(shell: &ShellState) -> Result<(), String> {
    require(shell)?;
    let e = entry(shell)
        .filter(|e| {
            e.stage == Stage::Uncertain
                && target(&e.target)
                && matches!(e.intent, Some(Intent::LaneOrder(_)))
        })
        .ok_or("No uncertain lane move")?;
    ledger()
        .lock()
        .unwrap()
        .queue_precision_file(e.original.raw)
}
pub(crate) fn seed_reopen(
    shell: &mut ShellState,
    visit: u64,
    loaded: crate::slack_lists::Loaded,
    find: crate::slack_lists::find::State,
) -> Result<(), String> {
    require(shell)?;
    if ledger().lock().unwrap().held_read.is_none() {
        return Err("No held lane move read".into());
    }
    seed_reopened(shell, visit, loaded, find)
}
/// Read-only proof that releasing the retired first metadata read did not
/// dispatch the remaining permission reads under its obsolete owner.
pub(crate) fn retired_read_receipt(shell: &ShellState, held: &Value) -> Result<Value, String> {
    require(shell)?;
    let now = snapshot(shell)?;
    if held["requests"].as_array().map(Vec::len) != Some(8)
        || held["responses"].as_array().map(Vec::len) != Some(8)
        || held["finished"].as_array().map(Vec::len) != Some(2)
        || held["requests"][7]["path"] != "/api/files.info"
        || held["readPending"] != true
        || held["writePending"] != false
        || now["requests"] != held["requests"]
        || now["responses"] != held["responses"]
        || now["readPending"] != false
        || now["writePending"] != false
        || now["finished"].as_array().map(Vec::len) != Some(3)
        || now["finished"][0] != held["finished"][0]
        || now["finished"][1] != held["finished"][1]
        || now["finished"][2]["kind"] != "check"
        || now["finished"][2]["serial"] != held["finished"][0]["serial"]
        || now["finished"][2]["applied"] != false
        || now["server"] != held["server"]
    {
        return Err(
            "Retired lane move read issued more requests or changed the replacement".into(),
        );
    }
    Ok(
        json!({"formsAtHold":8,"formsAfterRetirement":8,"retiredReadPaths":["/api/files.info"],
        "noFurtherPolicyReads":true,"completion":now["finished"][2]}),
    )
}
fn seed_reopened(
    shell: &mut ShellState,
    visit: u64,
    loaded: crate::slack_lists::Loaded,
    find: crate::slack_lists::find::State,
) -> Result<(), String> {
    let e = entry(shell)
        .filter(|e| {
            e.stage == Stage::Checking && target(&e.target) && e.intent.is_some() && e.job.is_some()
        })
        .ok_or("Missing retained lane move check")?;
    if generation(shell) != Some(visit)
        || e.original != loaded.document
        || e.original_records.as_deref() != Some(&loaded.records)
        || e.view_selection == selection(shell)
        || find.input != "Launch item 0"
        || find.applied != find.input
        || find.revision == 0
    {
        return Err("Wrong captured lane move snapshot/Find/replacement".into());
    }
    let v = shell
        .slack_lists
        .view
        .as_mut()
        .ok_or("Missing replacement List")?;
    if v.id != loaded.document.id
        || v.loaded.is_some()
        || v.loading
        || v.unavailable
        || v.read != 0
        || v.revision != 0
        || !v.layout.find.input.is_empty()
        || !v.layout.find.applied.is_empty()
    {
        return Err("Replacement List already populated".into());
    }
    v.title = loaded.document.title.clone();
    v.loaded = Some(loaded);
    v.stale = false;
    v.revision = next_id();
    v.layout.find = find;
    crate::slack_lists::rebuild_rows(shell);
    Ok(())
}
pub(super) fn read_form(request: &PreparedRequest) -> Result<(), Error> {
    let path =
        url::Url::parse(&request.url).map_err(|_| Error::Api("fixture_reorder_url".into()))?;
    let fields = match path.path() {
        "/api/files.info" => vec![("file", "F_LIST_FIXTURE")],
        "/api/team.prefs.get" => vec![
            ("target_team", "T1"),
            ("_x_reason", "native-list-permission"),
        ],
        "/api/users.prefs.get" => vec![
            ("prefs", "task_lists,assigned_to_list"),
            ("_x_reason", "native-list-completion-permission"),
        ],
        _ => return Err(Error::Api("fixture_reorder_read_path".into())),
    };
    let expected = std::iter::once(("token".to_owned(), "fixture-not-a-token".to_owned()))
        .chain(
            fields
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v.to_owned())),
        )
        .collect::<Vec<_>>();
    if !matches!(&request.body,RequestBody::Form(fields) if fields==&expected) {
        return Err(Error::Api("fixture_reorder_read_form".into()));
    }
    Ok(())
}
fn set_grouping(server: &mut Value) -> Result<(), Error> {
    let target = server["list_metadata"]["views"]
        .as_array_mut()
        .ok_or_else(|| Error::Api("fixture_lane_views".into()))?
        .iter_mut()
        .find(|v| v["id"] == "V_LANE")
        .ok_or_else(|| Error::Api("fixture_lane_id".into()))?;
    let mut original = expected_grouping();
    original["order"] = json!([{"select":[]},{"select":["OPT_PROGRESS"]},{"select":["OPT_DONE"]}]);
    if target["grouping"] != original {
        return Err(Error::Api("fixture_lane_original".into()));
    }
    target["grouping"] = expected_grouping();
    Ok(())
}
pub(super) async fn write(
    state: Signal<ShellState>,
    guard: &Guard,
    permit: Permit,
) -> Result<Value, ListColumnRenameFailure> {
    use ListColumnRenameFailure::{NotRequested, Unconfirmed};
    if !check_enabled()
        || !enabled(&state.peek())
        || !guard.current(&state.peek(), Stage::Saving)
        || registry::blocked(&guard.key.owner)
        || !permit.is_lane_order()
    {
        return Err(NotRequested(Error::TransportNotConfigured));
    }
    let original = with_registry(|r| {
        r.entries.get(&guard.key).and_then(|e| match &e.intent {
            Some(Intent::LaneOrder(i))
                if target(&e.target) && i.grouping() == &expected_grouping() =>
            {
                Some(i.original().raw.clone())
            }
            _ => None,
        })
    })
    .ok_or(NotRequested(Error::TransportNotConfigured))?;
    {
        let l = ledger().lock().unwrap();
        if l.pending.is_some()
            || l.held_read.is_some()
            || l.requests.len() != 3
            || l.responses.len() != 3
            || l.server.as_ref() != Some(&original)
        {
            return Err(NotRequested(Error::Api("fixture_reorder_order".into())));
        }
    }
    let request = permit.request(&state.peek().core.client, &session());
    let expected = vec![
        ("token".into(), "fixture-not-a-token".into()),
        ("list_id".into(), "F_LIST_FIXTURE".into()),
        ("id".into(), "V_LANE".into()),
        (
            "grouping".into(),
            serde_json::to_string(&expected_grouping()).unwrap(),
        ),
        ("_x_reason".into(), "update-view".into()),
    ];
    if !matches!(&request.body,RequestBody::Form(fields) if fields==&expected) {
        return Err(NotRequested(Error::Api("fixture_reorder_form".into())));
    }
    if record(request).map_err(NotRequested)? != "/api/lists.views.update" {
        return Err(NotRequested(Error::Api("fixture_reorder_path".into())));
    }
    let (tx, rx) = tokio::sync::oneshot::channel();
    {
        let mut l = ledger().lock().unwrap();
        l.responses
            .push(json!({"syntheticFailure":"response_lost"}));
        l.pending = Some(tx);
    }
    tokio::time::timeout(std::time::Duration::from_secs(30), rx)
        .await
        .map_err(|_| Unconfirmed(Error::Transport("fixture_timeout".into())))?
        .map_err(|_| Unconfirmed(Error::Transport("fixture_closed".into())))?;
    set_grouping(ledger().lock().unwrap().server.as_mut().unwrap()).map_err(Unconfirmed)?;
    Err(Unconfirmed(Error::Transport(
        "fixture_response_lost".into(),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn changed_find_revision_refuses_the_captured_direct_move_before_registry_entry() {
        let mut shell = ShellState::fixture_variant(
            crate::media::MediaRegistry::default(),
            "slack-list-board-lane-order",
        );
        let action = super::super::super::lane_order::adjacent(
            &shell,
            "OPT_DONE",
            super::super::super::lane_order::Direction::Left,
        )
        .unwrap();
        let before = shell
            .slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .clone()
            .unwrap();
        shell
            .slack_lists
            .view
            .as_mut()
            .unwrap()
            .layout
            .find
            .revision += 1;
        assert!(!super::super::super::lane_order::open_shell(
            &mut shell, action
        ));
        assert!(entry(&shell).is_none());
        let after = shell
            .slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap();
        assert_eq!(after.document, before.document);
        assert_eq!(after.records, before.records);
    }
    #[test]
    fn gate_requires_exact_flag_and_owned_alias() {
        for alias in [
            "slack-list-board-lane-order",
            "slack-list-board-lane-order-narrow",
        ] {
            assert!(gate(Some("1"), Some(alias)));
            assert!(!gate(None, Some(alias)));
            assert!(!gate(Some("true"), Some(alias)));
        }
        assert!(!gate(Some("1"), Some("slack-list-board-move")));
        assert!(!gate(Some("1"), Some("slack-list-view-order")));
    }
    #[test]
    fn synthetic_server_only_changes_requested_grouping_and_refuses_second_apply() {
        let mut original = expected_grouping();
        original["order"] =
            json!([{"select":[]},{"select":["OPT_PROGRESS"]},{"select":["OPT_DONE"]}]);
        let mut raw = json!({"id":"F_LIST_FIXTURE","list_metadata":{"schema":[{"opaque":true}],"views":[
            {"id":"OTHER","name":"Other","opaque":[null,true]},
            {"id":"V_LANE","grouping":original,"opaque":{"keep":1}}
        ],"opaque":[7]},"outside":{"keep":true}});
        let before = raw.clone();
        set_grouping(&mut raw).unwrap();
        assert_eq!(
            raw["list_metadata"]["views"][1]["grouping"],
            expected_grouping()
        );
        assert!(set_grouping(&mut raw).is_err());
        raw["list_metadata"]["views"][1]["grouping"] =
            before["list_metadata"]["views"][1]["grouping"].clone();
        assert_eq!(raw, before);
    }
}
