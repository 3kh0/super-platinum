//! Closed event ingress into the same shared collection used by production.
use super::*;
use crate::directory_member_fetches::Context;
use crate::profile_edit::sections::rich::search::file_source::reference;
use serde_json::json;
use std::sync::{Arc, Mutex, OnceLock};
use super_platinum_core::slack::events::{RawEvent, RtEvent};
pub(crate) const FLAG: &str = "SUPER_PLATINUM_PROFILE_CANVAS_DELETION_EVENT_CHECK";
pub(crate) const CASE: &str = "SUPER_PLATINUM_PROFILE_CANVAS_DELETION_EVENT_CASE";
pub(crate) const NORMAL: &str = "Known canvas";
pub(crate) const DELETED: &str = "Deleted canvas";
pub(crate) const SECOND: &str = "FSECOND01";
pub(crate) const ABSENT: &str = "FABSENT01";
pub(crate) fn case() -> String {
    std::env::var(CASE).unwrap_or_default()
}
pub(crate) fn named() -> bool {
    cfg!(any(test, feature = "protected-native-fixture"))
        && mentions::named()
        && [FLAG, mentions::FLAG, files::FLAG, files::BLOCKS_FLAG]
            .iter()
            .all(|f| std::env::var(f).as_deref() == Ok("1"))
        && std::env::var("SUPER_PLATINUM_PROFILE_FILE_TITLES_KIND").as_deref() != Ok("list")
        && matches!(
            case().as_str(),
            "known" | "mounted" | "held" | "retirement" | "plural"
        )
        && [
            deleted_reference::FLAG,
            unavailable_reference::FLAG,
            group_title::FLAG,
            missing_group::FLAG,
            channel_title::FLAG,
            missing_channel::FLAG,
            user_title::FLAG,
            emoji_title::FLAG,
            link_title::FLAG,
            empty_canvas::FLAG,
            completion::FLAG,
            group_recovery::FLAG,
        ]
        .iter()
        .all(|f| std::env::var(f).as_deref() != Ok("1"))
}
pub(crate) fn response() -> Value {
    json!({"ok":true,"file":{"id":files::ID,"mode":"quip","filetype":"quip","title":NORMAL,"updated":101,"opaque":{"retain":true}}})
}
fn ledger() -> &'static Mutex<Vec<Value>> {
    static LEDGER: OnceLock<Mutex<Vec<Value>>> = OnceLock::new();
    LEDGER.get_or_init(Default::default)
}
pub(crate) struct EventOwner(Arc<Context>);
pub(crate) fn owner(shell: &ShellState) -> Result<EventOwner, String> {
    if !named() || !mentions::enabled(shell) {
        return Err("Closed event fixture required".into());
    }
    let team = shell.core.active_team.as_deref().ok_or("Missing team")?;
    Ok(EventOwner(Arc::new(
        Context::capture(shell, team).map_err(|e| e.to_string())?,
    )))
}
pub(crate) fn event(
    shell: &mut ShellState,
    owner: &EventOwner,
    raw: Value,
) -> Result<Value, String> {
    if !named() {
        return Err("Closed event fixture required".into());
    }
    let team = &owner.0.session.team_id;
    let before = reference::fixture_proof(shell, team, &[files::ID, SECOND, ABSENT]);
    let accepted = owner.0.current(shell);
    let event: RawEvent = serde_json::from_value(raw.clone()).map_err(|e| e.to_string())?;
    if accepted {
        let event = RtEvent::Unknown(event);
        let read = crate::file_events::prepare(shell, team, owner.0.realtime_generation(), &event);
        if read.is_some() {
            return Err("Deletion scheduled metadata read".into());
        }
        crate::realtime::apply_in_view(
            &mut shell.core,
            team,
            owner.0.realtime_generation(),
            event,
            true,
        );
        shell.refresh_from_core();
    }
    let after = reference::fixture_proof(shell, team, &[files::ID, SECOND, ABSENT]);
    let proof = json!({"raw":raw,"acceptedOwner":accepted,"generation":owner.0.realtime_generation(),"before":before,"after":after});
    ledger().lock().unwrap().push(proof.clone());
    Ok(proof)
}
pub(crate) fn snapshot(shell: &ShellState) -> Value {
    let team = shell.core.active_team.as_deref().unwrap_or_default();
    json!({"case":case(),"account":shell.core.active_account,"epoch":shell.core.account_epoch,"socketGeneration":shell.core.workspaces.get(team).map(|w|w.rt_generation),"files":files::status(),"events":ledger().lock().unwrap().clone(),"shared":reference::fixture_proof(shell,team,&[files::ID,SECOND,ABSENT]),"networkCalls":0})
}
/// Reopen through the existing pure Profile and section load boundaries. Only
/// the disconnected capability/readback response is supplied by this adapter.
pub(crate) fn reopen(shell: &mut ShellState) -> Result<Value, String> {
    if !named() || !mentions::enabled(shell) {
        return Err("Closed event fixture required".into());
    }
    let before = reference::fixture_proof(
        shell,
        shell.core.active_team.as_deref().ok_or("Missing team")?,
        &[files::ID, SECOND, ABSENT],
    );
    let (token, visit) = crate::profile_edit::open_shell(shell).ok_or("Profile reopen refused")?;
    let cap = crate::profile_edit::fixture::capabilities(shell, &token.owner);
    crate::profile_edit::finish_load(shell, &token, visit, Ok(cap));
    let (token, serial) = super::super::open_shell(shell, visit).ok_or("Section reopen refused")?;
    super::interaction::load(shell, &token, visit, serial);
    let pane = shell
        .profile_edit
        .sections
        .draft
        .as_mut()
        .ok_or("Missing reopened pane")?;
    pane.form = Some(
        protocol::SectionDraft::open(
            pane.snapshot.as_ref().ok_or("Missing reopened snapshot")?,
            "S1",
        )
        .map_err(|e| e.to_string())?,
    );
    let after = reference::fixture_proof(
        shell,
        shell.core.active_team.as_deref().ok_or("Missing team")?,
        &[files::ID, SECOND, ABSENT],
    );
    if before != after {
        return Err("Reopen changed shared file source".into());
    }
    Ok(json!({"newVisit":visit,"newSerial":serial,"sharedUnchanged":true}))
}

pub(crate) fn retire_socket(mut state: Signal<ShellState>) -> Result<Value, String> {
    if !named() || !mentions::enabled(&state.peek()) {
        return Err("Closed event fixture required".into());
    }
    let mut shell = state.write();
    let team = shell.core.active_team.clone().ok_or("Missing team")?;
    let w = shell
        .core
        .workspaces
        .get_mut(&team)
        .ok_or("Missing workspace")?;
    let before = w.rt_generation;
    w.rt_generation += 1;
    Ok(json!({"before":before,"after":w.rt_generation}))
}

pub(crate) const OTHER_TEAM: &str = "TEVENTOTHER";
pub(crate) fn seed_navigation(shell: &mut ShellState) -> Result<(), String> {
    if !named() || !mentions::enabled(shell) || case() != "known" {
        return Err("Closed known navigation fixture required".into());
    }
    let team = shell.core.active_team.clone().ok_or("Missing team")?;
    if !reference::fixture_proof(shell, &team, &[files::ID]).is_null() {
        return Err("Navigation workspace must precede source publication".into());
    }
    let mut session = shell
        .core
        .session
        .as_ref()
        .ok_or("Missing auth")?
        .workspaces[&team]
        .clone();
    session.team_id = OTHER_TEAM.into();
    session.name = "Other fixture workspace".into();
    let mut workspace = shell.core.workspaces[&team].clone();
    workspace.team_id = OTHER_TEAM.into();
    workspace.name = "Other fixture workspace".into();
    shell
        .core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .insert(OTHER_TEAM.into(), session);
    shell.core.workspaces.insert(OTHER_TEAM.into(), workspace);
    // refresh_from_core projects the active conversation, not the workspace
    // rail. Seed that same session-backed rail before any source observation;
    // select_workspace must use its real destination entry below.
    let session = &shell.core.session.as_ref().unwrap().workspaces[OTHER_TEAM];
    shell.workspaces.push(crate::state::WorkspaceVm {
        id: session.team_id.clone(),
        name: session.name.clone(),
        initials: session
            .name
            .chars()
            .next()
            .unwrap_or('S')
            .to_uppercase()
            .to_string(),
    });
    if [team.as_str(), OTHER_TEAM].iter().any(|id| {
        !shell
            .core
            .session
            .as_ref()
            .unwrap()
            .workspaces
            .contains_key(*id)
            || !shell.core.workspaces.contains_key(*id)
            || shell.workspaces.iter().filter(|row| row.id == *id).count() != 1
    }) || shell
        .workspaces
        .get(shell.active_workspace)
        .map(|row| row.id.as_str())
        != Some(team.as_str())
    {
        return Err("Navigation fixture inventories disagree".into());
    }
    shell.refresh_from_core();
    Ok(())
}
pub(crate) fn navigate(
    shell: &mut ShellState,
    owner: &EventOwner,
    away: bool,
) -> Result<Value, String> {
    if !named() || case() != "known" || !owner.0.account_current(shell) {
        return Err("Original navigation credentials required".into());
    }
    let original = &owner.0.session.team_id;
    let target = if away { OTHER_TEAM } else { original.as_str() };
    let before = reference::fixture_proof(shell, original, &[files::ID, SECOND, ABSENT]);
    let old_generation = shell.profile_generation;
    let index = shell
        .workspaces
        .iter()
        .position(|w| w.id == target)
        .ok_or("Missing navigation destination")?;
    if !shell.select_workspace(index) || !owner.0.account_current(shell) {
        return Err("Workspace navigation lost credentials".into());
    }
    let after = reference::fixture_proof(shell, original, &[files::ID, SECOND, ABSENT]);
    if before != after {
        return Err("Workspace navigation changed event facts".into());
    }
    if !away {
        mentions::rebind_deletion_navigation(shell, &owner.0)?;
    }
    Ok(
        json!({"target":target,"activeTeam":shell.core.active_team,"before":before,"after":after,"sharedUnchanged":true,"credentialsCurrent":true,"profileGenerationBefore":old_generation,"profileGenerationAfter":shell.profile_generation}),
    )
}
