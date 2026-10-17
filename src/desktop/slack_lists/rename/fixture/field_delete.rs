//! Exact disconnected deletion adapter over the shared ledger and production worker.
use super::*;

pub(crate) fn check_enabled() -> bool {
    gate(
        std::env::var("SUPER_PLATINUM_LIST_FIELD_DELETE_CHECK")
            .ok()
            .as_deref(),
        std::env::var("SUPER_PLATINUM_FIXTURE").ok().as_deref(),
    )
}
fn gate(flag: Option<&str>, alias: Option<&str>) -> bool {
    flag == Some("1")
        && matches!(
            alias,
            Some("slack-list-field-delete" | "slack-list-field-delete-narrow")
        )
}
fn require(shell: &ShellState) -> Result<(), String> {
    if check_enabled() && enabled(shell) {
        Ok(())
    } else {
        Err("Exact disconnected field deletion checker required".into())
    }
}
pub(crate) fn seed(shell: &ShellState, server: Value) {
    require(shell).unwrap();
    *ledger().lock().unwrap() = Ledger {
        server: Some(server),
        ..Default::default()
    };
}
/// Static fixtures may review and Cancel; they do not initialize the HTTP ledger.
pub(crate) fn open_static(shell: &mut ShellState) -> bool {
    if !crate::slack_lists::fixture::enabled(shell) {
        return false;
    }
    let Some(visit) = generation(shell) else {
        return false;
    };
    let Some(selected) = selection(shell) else {
        return false;
    };
    if !super::super::field_delete::open_shell(shell, visit, selected) {
        return false;
    }
    let Some(e) = entry(shell) else {
        return false;
    };
    super::super::field_delete::choose_shell(shell, e.serial, visit, "C_HIDDEN")
}
pub(crate) fn registry_empty(shell: &ShellState) -> Result<bool, String> {
    require(shell)?;
    Ok(entry(shell).is_none() && super::super::presentation(shell).is_none())
}
pub(crate) fn custody(shell: &ShellState) -> Result<Value, String> {
    require(shell)?;
    let e = entry(shell)
        .filter(|e| e.target.is_deletion())
        .ok_or("Missing deletion custody")?;
    Ok(
        json!({"account":shell.core.active_account,"epoch":shell.core.account_epoch,"serial":e.serial,
        "target":e.target.id(),"original":e.original.raw,"population":e.original_records.as_ref().map(|r| &r.raw),
        "intent":e.intent.as_ref().map(|i|match i {Intent::DeleteColumn(i)=>json!({"original":i.original().raw,"column":i.column_id(),"key":i.column_key()}),_=>Value::Null})}),
    )
}
pub(crate) fn refuse_old_confirmation(
    shell: &mut ShellState,
    old_serial: u64,
) -> Result<Value, String> {
    require(shell)?;
    let e = entry(shell)
        .filter(|e| e.stage == Stage::Editing && e.target.is_deletion() && e.serial != old_serial)
        .ok_or("No replaced deletion choice")?;
    let before = custody(shell)?;
    let prepared = super::super::prepare(shell, old_serial).is_some();
    let closed = super::super::close_entry(shell, old_serial);
    if prepared
        || closed
        || custody(shell)? != before
        || entry(shell)
            .is_none_or(|a| a.serial != e.serial || a.job.is_some() || a.intent.is_some())
    {
        return Err("Old deletion callback changed authority".into());
    }
    Ok(
        json!({"oldSerial":old_serial,"currentSerial":e.serial,"confirm":prepared,"cancel":closed,"custody":before}),
    )
}
pub(crate) fn policy_refusals(shell: &mut ShellState) -> Result<Value, String> {
    require(shell)?;
    let e = entry(shell)
        .filter(|e| {
            e.stage == Stage::Editing
                && e.target.is_deletion()
                && e.job.is_none()
                && e.intent.is_none()
        })
        .ok_or("Deletion must still be reviewing")?;
    let before = custody(shell)?;
    let before_ledger = snapshot(shell)?;
    let user = super::super::text_create::user(shell)
        .ok_or("Missing authenticated fixture user")?
        .to_owned();
    let loaded = shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap();
    let flags = loaded
        .list_experiments
        .ok_or("Missing original experiment assignment")?;
    if flags.schema_owner_only
        || loaded.document.raw["user"] == user
        || loaded.document.raw["access"] != "write"
    {
        return Err("Expected experiment-off collaborator fixture".into());
    }
    let off = super::super::field_delete::capture(shell, &e, loaded).is_ok();
    let result = (|| {
        let mut cases = Vec::new();
        for assignment in [Some(true), None] {
            shell
                .slack_lists
                .view
                .as_mut()
                .unwrap()
                .loaded
                .as_mut()
                .unwrap()
                .list_experiments =
                assignment.map(|on| super_platinum_core::slack::api::ListExperiments {
                    schema_owner_only: on,
                    ..flags
                });
            let can_confirm = super::super::presentation(shell).is_some_and(|p| p.can_confirm);
            let prepared = super::super::prepare(shell, e.serial).is_some();
            if can_confirm || prepared {
                return Err("Unverified or owner-only collaborator deletion admitted".to_owned());
            }
            cases.push(
                json!({"schemaOwnerOnly":assignment,"canConfirm":can_confirm,"prepared":prepared}),
            );
        }
        Ok(cases)
    })();
    // This explicit fixture setup restores only policy and review copy. It never
    // clears an admitted job or reconstructs a permit after unexpected admission.
    shell
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .loaded
        .as_mut()
        .unwrap()
        .list_experiments = Some(flags);
    if let Some(key) = Key::current(shell) {
        with_registry(|r| {
            if let Some(current) = r.entries.get_mut(&key).filter(|c| {
                c.serial == e.serial
                    && c.stage == Stage::Editing
                    && c.job.is_none()
                    && c.intent.is_none()
            }) {
                current.message = e.message.clone();
            }
        });
    }
    let cases = result?;
    if !off
        || custody(shell)? != before
        || snapshot(shell)? != before_ledger
        || super::super::text_create::user(shell) != Some(user.as_str())
        || !super::super::presentation(shell).is_some_and(|p| p.can_confirm)
    {
        return Err("Policy refusal did not restore exact review".into());
    }
    Ok(
        json!({"user":user,"owner":e.original.raw["user"],"offAllowed":off,"refused":cases,"restored":true,"ledgerUnchanged":true,"custody":before}),
    )
}
pub(crate) fn conflict_file(shell: &ShellState) -> Result<(), String> {
    require(shell)?;
    let e = entry(shell)
        .filter(|e| {
            e.stage == Stage::Uncertain
                && e.target == Target::DeleteColumn("C_HIDDEN".into())
                && matches!(e.intent, Some(Intent::DeleteColumn(_)))
        })
        .ok_or("No uncertain deletion")?;
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
        return Err("No held deletion read".into());
    }
    seed_reopened(shell, visit, loaded, find)
}
/// Read-only proof that releasing the retired first metadata read did not
/// dispatch the remaining permission reads under its obsolete owner.
pub(crate) fn retired_read_receipt(shell: &ShellState, held: &Value) -> Result<Value, String> {
    require(shell)?;
    let now = snapshot(shell)?;
    if held["requests"].as_array().map(Vec::len) != Some(10)
        || held["responses"].as_array().map(Vec::len) != Some(10)
        || held["finished"].as_array().map(Vec::len) != Some(2)
        || held["requests"][9]["path"] != "/api/files.info"
        || held["readPending"] != true
        || held["writePending"] != false
        || now["requests"] != held["requests"]
        || now["responses"] != held["responses"]
        || now["readPending"] != false
        || now["writePending"] != false
        || now["finished"].as_array().map(Vec::len) != Some(3)
        || now["finished"][2]["applied"] != false
    {
        return Err("Retired deletion read issued more requests or changed the replacement".into());
    }
    Ok(
        json!({"formsAtHold":10,"formsAfterRetirement":10,"retiredReadPaths":["/api/files.info"],
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
            e.stage == Stage::Checking
                && e.target == Target::DeleteColumn("C_HIDDEN".into())
                && e.intent.is_some()
                && e.job.is_some()
        })
        .ok_or("Missing retained deletion check")?;
    if generation(shell) != Some(visit)
        || e.original != loaded.document
        || e.original_records.as_deref() != Some(&loaded.records)
        || e.view_selection == selection(shell)
        || find.input != "Launch item 0"
        || find.applied != find.input
        || find.revision == 0
    {
        return Err("Wrong captured deletion snapshot/Find/replacement".into());
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
pub(super) fn policy_response(request: &PreparedRequest) -> Result<Option<Value>, Error> {
    if url::Url::parse(&request.url)
        .ok()
        .is_none_or(|u| u.path() != "/api/experiments.getByUser")
    {
        return Ok(None);
    }
    let expected = vec![
        ("token".into(), "fixture-not-a-token".into()),
        (
            "ssb_platform".into(),
            std::env::consts::OS.replace("macos", "darwin"),
        ),
        ("ssb_release_channel".into(), "prod".into()),
    ];
    if !matches!(&request.body,RequestBody::Form(fields) if fields==&expected) {
        return Err(Error::Api("fixture_delete_experiments_form".into()));
    }
    Ok(Some(
        json!({"ok":true,"assignments":{"lists_schema_owner_only":{"group":"off"}}}),
    ))
}
fn delete_field(server: &mut Value) -> Result<(), Error> {
    let schema = server["list_metadata"]["schema"]
        .as_array_mut()
        .ok_or_else(|| Error::Api("fixture_delete_schema".into()))?;
    if schema
        .iter()
        .filter(|c| c["id"] == "C_HIDDEN" && c["key"] == "hidden")
        .count()
        != 1
    {
        return Err(Error::Api("fixture_delete_target".into()));
    }
    schema.retain(|c| c["key"] != "hidden");
    for view in server["list_metadata"]["views"]
        .as_array_mut()
        .ok_or_else(|| Error::Api("fixture_delete_views".into()))?
    {
        view["columns"]
            .as_array_mut()
            .ok_or_else(|| Error::Api("fixture_delete_view_columns".into()))?
            .retain(|c| c["key"] != "hidden");
    }
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
        || !permit.is_deletion()
    {
        return Err(NotRequested(Error::TransportNotConfigured));
    }
    let original = with_registry(|r| {
        r.entries.get(&guard.key).and_then(|e| match &e.intent {
            Some(Intent::DeleteColumn(i))
                if e.target == Target::DeleteColumn("C_HIDDEN".into())
                    && i.column_key() == "hidden" =>
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
            || l.requests.len() != 4
            || l.responses.len() != 4
            || l.server.as_ref() != Some(&original)
        {
            return Err(NotRequested(Error::Api("fixture_delete_order".into())));
        }
    }
    let request = permit.request(&state.peek().core.client, &session());
    let expected = vec![
        ("token".into(), "fixture-not-a-token".into()),
        ("list_id".into(), "F_LIST_FIXTURE".into()),
        ("key".into(), "hidden".into()),
        ("_x_reason".into(), "delete-column".into()),
    ];
    if !matches!(&request.body,RequestBody::Form(fields) if fields==&expected) {
        return Err(NotRequested(Error::Api("fixture_delete_form".into())));
    }
    if record(request).map_err(NotRequested)? != "/api/lists.columns.delete" {
        return Err(NotRequested(Error::Api("fixture_delete_path".into())));
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
    delete_field(ledger().lock().unwrap().server.as_mut().unwrap()).map_err(Unconfirmed)?;
    Err(Unconfirmed(Error::Transport(
        "fixture_response_lost".into(),
    )))
}
#[cfg(test)]
pub(crate) fn close_static_for_test(shell: &ShellState) -> bool {
    let Some(e) = entry(shell).filter(|e| {
        e.target.is_deletion() && e.stage == Stage::Editing && e.job.is_none() && e.intent.is_none()
    }) else {
        return false;
    };
    super::super::close_entry(shell, e.serial)
}
#[cfg(test)]
mod tests;
