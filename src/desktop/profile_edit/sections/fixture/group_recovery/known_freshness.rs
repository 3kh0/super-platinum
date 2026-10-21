//! Closed known-group freshness inputs. All freshness stamps come from the
//! production source sink; typed group presence is never treated as a stamp.
use super::*;
use crate::usergroups::source::{self, Admission, NativeMembers, Operation, Origin};
use std::sync::Arc;
use super_platinum_core::slack::realtime::Connection;
use tokio::sync::mpsc;

pub(crate) const RENAMED: &str = "@Renamed crew";
pub(crate) fn enabled() -> bool {
    super::enabled()
        && matches!(
            case().as_str(),
            "known-freshness" | "known-freshness-errors" | "known-freshness-retirement"
        )
}
#[derive(Default)]
struct Evidence {
    seed: Option<Value>,
    resets: Vec<Value>,
    receivers: Vec<mpsc::Receiver<String>>,
}
fn evidence() -> &'static Mutex<Evidence> {
    static BOOK: OnceLock<Mutex<Evidence>> = OnceLock::new();
    BOOK.get_or_init(Mutex::default)
}
pub(crate) fn raw(id: &str, renamed: bool, equal_poison: bool) -> Value {
    let mut row = super::raw(id);
    row["date_update"] = json!(if id == GROUP && renamed { 102 } else { 101 });
    if id == GROUP && renamed {
        row["handle"] = json!(if equal_poison {
            "Equal version must not replace"
        } else {
            "Renamed crew"
        });
    }
    row
}
pub(crate) fn response(renamed: bool, equal_poison: bool) -> Value {
    json!({"ok":true,"results":ids().iter().map(|id|raw(id,renamed,equal_poison)).collect::<Vec<_>>()})
}
pub(crate) async fn seed(mut state: Signal<ShellState>) -> Result<(), String> {
    if !enabled() || evidence().lock().unwrap().seed.is_some() {
        return Err("Fresh closed known-group seed required".into());
    }
    let owner = Owner::capture(&state.peek()).ok_or("Missing known-group owner")?;
    let user = json!({"id":owner.user,"name":"fixture-self","team_id":owner.team,"profile":{}});
    let boot = json!({"ok":true,"team":{"id":owner.team,"name":"Known group freshness"},
        "self":user,"users":[user],"channels":[{"id":"CINIT0001","name":"warm-harbor",
        "is_channel":true,"is_member":true}],"prefs":{}});
    let init = json!({"ok":true,"self":user,"team":{"id":owner.team,"name":"Known group freshness"},
        "workspaces":[{"id":owner.team}],"onboarding":{"user_data":{"onboarding_complete":true}},
        "user_date_created":123});
    crate::app_home::channel_fetches::canonical::fixture_seed(state, boot.clone(), init.clone())
        .await?;
    let context = Arc::new(
        Context::capture(&state.peek(), &owner.team)
            .map_err(|_| "Missing original group source context")?,
    );
    let rows = ids()
        .iter()
        .map(|id| raw(id, false, false))
        .collect::<Vec<_>>();
    let work = source::enqueue(
        &mut state.write(),
        context.clone(),
        Operation::Metadata {
            origin: Origin::Fetch,
            rows: rows.clone(),
            native_members: NativeMembers::Preserve,
        },
    );
    let Admission::Tracked(work) = work else {
        return Err("Known-group seed has no genuine canonical source lineage".into());
    };
    work.run(state, |shell| context.current(shell))
        .await
        .map_err(|e| format!("Known-group original source seed: {e}"))?;
    evidence().lock().unwrap().seed = Some(json!({
        "boot":boot,"init":init,"rows":rows,"origin":"Fetch",
        "nativeMembers":"Preserve","actualSourceSink":true
    }));
    Ok(())
}
fn lifecycle(shell: &mut ShellState, team: &str) -> Result<Value, String> {
    let observed = crate::app_home::cfm_lifecycle::observed(shell, team)
        .map_err(|_| "Known-group lifecycle is not positively observed")?;
    Ok(json!({"slowConnectionCount":observed.slow_connections,"major":observed.major}))
}
/// Reuse the accepted closed socket producer, with a real Connection and a
/// retained receiver. This is not a direct scheduler timestamp/reset setter.
pub(crate) fn reconnect(shell: &mut ShellState) -> Result<Value, String> {
    if !enabled() {
        return Err("Closed known-group reconnect required".into());
    }
    let team = shell.core.active_team.clone().ok_or("Missing reset team")?;
    let before = lifecycle(shell, &team)?;
    let workspace = shell
        .core
        .workspaces
        .get_mut(&team)
        .ok_or("Missing workspace")?;
    let old_generation = workspace.rt_generation;
    let generation = old_generation
        .checked_add(1)
        .ok_or("Socket generation exhausted")?;
    let (send, receive) = mpsc::channel(32);
    workspace.rt_generation = generation;
    workspace.rt =
        super_platinum_core::state::RealtimeStatus::Connected(Connection::from_sender(send));
    let _work = crate::app_home::channel_fetches::connected(shell, &team, generation)
        .ok_or("Closed accepted socket was refused")?;
    let after = lifecycle(shell, &team)?;
    // Repeating the same accepted generation must not invent another reconnect.
    crate::app_home::cfm_lifecycle::connected(shell, &team, generation);
    let duplicate = lifecycle(shell, &team)?;
    if after != duplicate
        || before["slowConnectionCount"]
            .as_u64()
            .and_then(|v| v.checked_add(1))
            != after["slowConnectionCount"].as_u64()
    {
        return Err("Accepted socket count or duplicate-generation guard changed".into());
    }
    let receipt = json!({"team":team,"before":before,"after":after,"duplicate":duplicate,
        "oldGeneration":old_generation,"generation":generation,"actualAcceptedConnection":true});
    let mut book = evidence().lock().unwrap();
    book.resets.push(receipt.clone());
    book.receivers.push(receive);
    Ok(receipt)
}
pub(crate) fn snapshot(shell: &ShellState) -> Value {
    let team = shell.core.active_team.as_deref().unwrap_or("");
    let observed = (|| {
        let account = shell.core.active_account.as_ref()?;
        let session = shell.core.session.as_ref()?.workspaces.get(team)?;
        let w = shell.core.workspaces.get(team)?;
        let source = w
            .directory_usergroups
            .snapshot(account, session, shell.core.account_epoch, &w.usergroups)
            .ok()?;
        Some(
            json!({"rows":source.rows(),"membership":source.membership(),
            "current":source.current(&w.directory_usergroups,account,session,shell.core.account_epoch,&w.usergroups)}),
        )
    })();
    let book = evidence().lock().unwrap();
    json!({"seed":book.seed,"resets":book.resets,"source":observed,
        "scheduler":crate::usergroups::freshness::snapshot(shell,team),
        "opening":shell.profile_edit.sections.draft.as_ref().map(|d|json!({
            "visit":d.visit,"serial":d.serial,"key":d.rich_opening,
            "mounted":d.rich.is_some(),"reading":d.rich_reading,"lease":d.rich_recovery.is_some()
        })),"networkCalls":0})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn equal_version_probe_preserves_version_but_changes_only_requested_handle() {
        let before = raw(GROUP, true, false);
        let mut poison = raw(GROUP, true, true);
        assert_eq!(before["date_update"], json!(102));
        assert_ne!(before["handle"], poison["handle"]);
        poison["handle"] = before["handle"].clone();
        assert_eq!(before, poison);
        assert_eq!(raw(OTHER, true, true), raw(OTHER, false, false));
    }
    #[test]
    fn original_known_fixture_handle_and_metadata_remain_untrimmed() {
        let original = raw(GROUP, false, false);
        assert_eq!(original["handle"], json!(LABEL));
        assert_eq!(original["date_update"], json!(101));
        assert_eq!(original["fixture_metadata"], json!({"keep":true}));
        assert_eq!(raw(GROUP, true, false)["handle"], json!("Renamed crew"));
    }
}
