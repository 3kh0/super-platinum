//! Closed mixed completion acceptance through actual native input and Save.
use super::*;
use super::{completion as source, completion_check as base, mentions, mentions_check as helper};
use dioxus::desktop::DesktopContext;
use serde_json::json;
use source::groups;
use std::time::Duration;
const PREFIX: &str = "profile-group-completion-check:";
const JS: &str = include_str!("group_completion_check.js");
const SHARED: &str = include_str!("member_completion_check.js");
const FAILURE: &str = "People suggestions could not load. Edit your query to try again.";
pub(crate) fn start(state: Signal<ShellState>, desktop: DesktopContext) {
    if !groups::named() {
        return;
    }
    let name = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap();
    dioxus::core::spawn_forever(async move {
        let result = match tokio::time::timeout(
            Duration::from_secs(190),
            run(state, &desktop, &name),
        )
        .await
        {
            Ok(Ok(v)) => v,
            Ok(Err(error)) => {
                json!({"ok":false,"fixture":name,"error":error,"model":mentions::snapshot(&state.peek()),"source":source::snapshot(&state.peek())})
            }
            Err(_) => {
                json!({"ok":false,"fixture":name,"error":"Group completion timed out","source":source::snapshot(&state.peek())})
            }
        };
        eprintln!("{PREFIX} {result}");
    });
}
fn initial() -> Vec<Value> {
    let mut blocks = base::initial();
    for i in 1..=2 {
        blocks[0]["elements"][0]["elements"][i]["opaque"] = json!({"keep":[i,"original"]});
    }
    blocks
}
fn choices(shell: &ShellState) -> Result<Value, String> {
    super::super::rich::fixture_choices(shell).ok_or("Missing original mixed choices".into())
}
fn source_rows(shell: &ShellState) -> Result<Value, String> {
    let team = shell.core.active_team.as_deref().ok_or("Missing team")?;
    let context = crate::directory_member_fetches::Context::capture(shell, team)
        .map_err(|e| e.to_string())?;
    let w = &shell.core.workspaces[team];
    let source = w
        .directory_usergroups
        .snapshot(
            &context.account,
            &context.session,
            context.epoch,
            &w.usergroups,
        )
        .map_err(|e| format!("Group source {e:?}"))?;
    Ok(
        json!({"rows":source.rows(),"membership":source.membership(),"selfRestricted":w.users.get(&context.session.user_id).map(|u|u.is_restricted)}),
    )
}
async fn choose(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    id: &str,
    keyboard: bool,
) -> Result<Value, String> {
    let expected = choices(&state.peek())?;
    helper::wait_dom(
        desktop,
        &format!(
            "({JS})({},({SHARED}))",
            json!({"id":id,"keyboard":keyboard,"choices":expected})
        ),
    )
    .await
}
async fn capture(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    stage: &str,
    index: Option<usize>,
    proof: Value,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    if let Some(index) = index {
        helper::wait_dom(
            desktop,
            &format!(
                "({JS})({},({SHARED}))",
                json!({"action":"scroll","index":index})
            ),
        )
        .await?;
    }
    let raw = base::JS;
    let geometry = helper::wait_dom(
        desktop,
        &format!(
            "({JS})({{action:'layout',index:{},geometry:({raw})('layout',null)}},({SHARED}))",
            json!(index)
        ),
    )
    .await
    .map_err(|e| format!("Group layout {stage}: {e}"))?;
    base::capture_measured(
        state, desktop, name, stage, proof, stages, captures, geometry,
    )
    .await
}
async fn mixed_ready(state: Signal<ShellState>, desktop: &DesktopContext) -> Result<Value, String> {
    base::rows(
        desktop,
        &[
            groups::LABEL,
            groups::SECOND_LABEL,
            "@Alex Local",
            "@Alex Retired",
            "@Alex Self",
        ],
        false,
    )
    .await?;
    let expected = choices(&state.peek())?;
    if expected["values"]
        .as_array()
        .is_none_or(|rows| rows.len() != 7)
    {
        return Err(format!("Expected seven mixed results: {expected}"));
    }
    for (kind, key, id) in [
        ("user", "user_id", source::members::FIRST),
        ("user", "user_id", source::members::SECOND),
        ("usergroup", "usergroup_id", groups::FIRST),
        ("usergroup", "usergroup_id", groups::SECOND),
    ] {
        if !expected["values"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["value"]["type"] == kind && v["value"][key] == id)
        {
            return Err(format!("Missing mixed identity {id}"));
        }
    }
    Ok(expected)
}
fn inserted(native: &Value, ids: &[&str]) -> Result<(), String> {
    let tokens = native["native"]["tokens"]
        .as_array()
        .ok_or("Missing native group custody")?;
    let actual = tokens
        .iter()
        .skip(2)
        .map(|t| {
            if t["value"]["type"] != "usergroup" {
                return Err("Inserted wrong category");
            }
            t["value"]["usergroup_id"]
                .as_str()
                .ok_or("Missing group identity")
        })
        .collect::<Result<Vec<_>, _>>()?;
    if actual != ids {
        return Err(format!("Group identity order {actual:?}"));
    }
    Ok(())
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    let readiness = helper::readiness(name).await?;
    if !groups::enabled(&state.peek()) || !mentions::user_mode() {
        return Err("Closed group fixture unavailable".into());
    }
    mentions::seed(state, "Alex Local").await?;
    let initial_source = source_rows(&state.peek())?;
    if !initial_source["rows"].as_array().is_some_and(Vec::is_empty) {
        return Err("Group source was not genuinely empty at initial Boot".into());
    }
    if initial_source["selfRestricted"] != groups::restricted() {
        return Err("Initial full Boot self restriction mismatch".into());
    }
    mentions::set_blocks(&mut state.write(), initial());
    helper::select_values(state, desktop).await?;
    helper::click(desktop, "Edit description", false).await?;
    base::tokens(desktop, 2).await?;
    let mut stages = vec![];
    let mut captures = serde_json::Map::new();
    capture(
        state,
        desktop,
        name,
        "opened",
        None,
        json!({"initialBlocks":initial(),"initialSource":initial_source}),
        &mut stages,
        &mut captures,
    )
    .await?;
    if groups::restricted() {
        base::query(desktop, "@alex").await?;
        base::rows(
            desktop,
            &["@Alex Local", "@Alex Harbor", "@Alex Retired", "@Alex Self"],
            false,
        )
        .await?;
        base::ledger(state, 0, 0).await?;
        let expected = choices(&state.peek())?;
        if expected["values"]
            .as_array()
            .is_none_or(|v| v.len() != 5 || v.iter().any(|r| r["value"]["type"] != "user"))
        {
            return Err("Restricted mixed query admitted groups".into());
        }
        if source::snapshot(&state.peek())["groupSearches"] != 0 {
            return Err("Restricted group read was dispatched".into());
        }
        let proof = json!({"choices":expected,"sourceGroups":source_rows(&state.peek())?,"noGroupRequest":true});
        for (stage, index) in [
            ("restricted", 0),
            ("restricted-middle", 2),
            ("restricted-bottom", 4),
        ] {
            capture(
                state,
                desktop,
                name,
                stage,
                Some(index),
                proof.clone(),
                &mut stages,
                &mut captures,
            )
            .await?;
        }
        return Ok(
            json!({"ok":true,"fixture":name,"restricted":true,"readiness":readiness,"stages":stages,"captures":captures,"initialBlocks":initial(),"source":source::snapshot(&state.peek()),"model":mentions::snapshot(&state.peek())}),
        );
    }
    groups::mode(&state.peek(), source::Mode::Held).map_err(|e| e.to_string())?;
    base::query(desktop, "@alex").await?;
    base::ledger(state, 1, 0).await?;
    let local = base::rows(
        desktop,
        &["@Alex Local", "@Alex Harbor", "@Alex Retired", "@Alex Self"],
        false,
    )
    .await?;
    let before_groups = choices(&state.peek())?;
    if before_groups["values"]
        .as_array()
        .is_none_or(|rows| rows.len() != 5 || rows.iter().any(|row| row["value"]["type"] != "user"))
    {
        return Err("Held group read did not retain current member results".into());
    }
    capture(
        state,
        desktop,
        name,
        "local",
        None,
        json!({"sequence":local["sequence"],"held":true,"choices":before_groups}),
        &mut stages,
        &mut captures,
    )
    .await?;
    let release = source::release(&state.peek()).map_err(|e| e.to_string())?;
    if release["delivered"] != true {
        return Err("Held group reply undelivered".into());
    }
    base::ledger(state, 0, 0).await?;
    let expected_choices = mixed_ready(state, desktop).await?;
    if expected_choices["sequence"] != local["sequence"] {
        return Err("Mixed publication replaced query".into());
    }
    let proof = json!({"sequence":local["sequence"],"release":release,"choices":expected_choices,"sourceGroups":source_rows(&state.peek())?});
    for (stage, index) in [
        ("refreshed", 0),
        ("refreshed-middle", 2),
        ("refreshed-lower", 4),
        ("refreshed-bottom", 6),
    ] {
        capture(
            state,
            desktop,
            name,
            stage,
            Some(index),
            proof.clone(),
            &mut stages,
            &mut captures,
        )
        .await?;
    }
    let choice = choose(state, desktop, groups::FIRST, true).await?;
    let native = base::tokens(desktop, 3).await?;
    inserted(&native, &[groups::FIRST])?;
    capture(
        state,
        desktop,
        name,
        "keyboard",
        None,
        json!({"group":groups::FIRST,"keyboard":true,"choice":choice}),
        &mut stages,
        &mut captures,
    )
    .await?;
    base::query(desktop, "@cafe").await?;
    base::rows(desktop, &[groups::SECOND_LABEL], false).await?;
    base::ledger(state, 0, 0).await?;
    let choice = choose(state, desktop, groups::SECOND, false).await?;
    let native = base::tokens(desktop, 4).await?;
    inserted(&native, &[groups::FIRST, groups::SECOND])?;
    let expected = native["native"]["snapshot"]["blocks"].clone();
    if native["native"]["tokens"][3]["label"] != groups::SECOND_LABEL {
        return Err("New insertion lost literal prefix/NFC".into());
    }
    capture(
        state,
        desktop,
        name,
        "mouse",
        None,
        json!({"group":groups::SECOND,"keyboard":false,"choice":choice,"expectedBlocks":expected}),
        &mut stages,
        &mut captures,
    )
    .await?;
    helper::click(desktop, "Use description", false).await?;
    helper::wait_dom(desktop,"({ok:!document.getElementById('profile-section-rich')&&!!document.getElementById('profile-section-field-long')})").await?;
    if mentions::snapshot(&state.peek())["blocks"] != expected {
        return Err("Use changed group blocks".into());
    }
    helper::save(state, desktop, Outcome::Saved, 1, 1).await?;
    helper::click(desktop, "Edit description", false).await?;
    let native = base::tokens(desktop, 4).await?;
    if native["native"]["snapshot"]["blocks"] != expected
        || native["native"]["tokens"][3]["label"] != groups::REOPENED_LABEL
    {
        return Err("Group readback violated existing-token contract".into());
    }
    capture(state,desktop,name,"saved",None,json!({"exactReadback":true,"expectedBlocks":expected,"reopenedLabel":groups::REOPENED_LABEL}),&mut stages,&mut captures).await?;
    groups::mode(&state.peek(), source::Mode::Held).map_err(|e| e.to_string())?;
    base::query(desktop, "@alex-held").await?;
    base::ledger(state, 1, 0).await?;
    base::query(desktop, "@alex").await?;
    mixed_ready(state, desktop).await?;
    base::ledger(state, 0, 1).await?;
    let release = source::release(&state.peek()).map_err(|e| e.to_string())?;
    if release["delivered"] != false || release["cancelledBeforeReply"] != true {
        return Err("Retired group read delivered".into());
    }
    capture(
        state,
        desktop,
        name,
        "query-retired",
        None,
        json!({"release":release,"tokensUnchanged":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    for (stage, term, mode) in [
        ("malformed", "@no-malformed", source::Mode::Malformed),
        ("failure", "@no-offline", source::Mode::Failure),
    ] {
        groups::mode(&state.peek(), mode).map_err(|e| e.to_string())?;
        base::query(desktop, term).await?;
        base::status(desktop, FAILURE).await?;
        base::ledger(state, 0, 1).await?;
        capture(
            state,
            desktop,
            name,
            stage,
            None,
            json!({"retainedBlocks":expected,"noWrite":true}),
            &mut stages,
            &mut captures,
        )
        .await?;
    }
    groups::mode(&state.peek(), source::Mode::Empty).map_err(|e| e.to_string())?;
    base::query(desktop, "@zz-no-matching-group").await?;
    base::status(desktop, "No matching people").await?;
    base::ledger(state, 0, 1).await?;
    capture(
        state,
        desktop,
        name,
        "empty",
        None,
        json!({"empty":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    groups::mode(&state.peek(), source::Mode::Held).map_err(|e| e.to_string())?;
    base::query(desktop, "@alex-cancel").await?;
    base::ledger(state, 1, 1).await?;
    helper::click(desktop, "Cancel description edit", false).await?;
    helper::wait_dom(
        desktop,
        "({ok:!document.getElementById('profile-section-rich')})",
    )
    .await?;
    base::ledger(state, 0, 2).await?;
    let release = source::release(&state.peek()).map_err(|e| e.to_string())?;
    if release["delivered"] != false
        || release["cancelledBeforeReply"] != true
        || mentions::snapshot(&state.peek())["blocks"] != expected
    {
        return Err("Group cancellation changed draft/delivered".into());
    }
    capture(
        state,
        desktop,
        name,
        "cancelled",
        None,
        json!({"release":release,"expectedBlocks":expected}),
        &mut stages,
        &mut captures,
    )
    .await?;
    helper::click(desktop, "Edit description", false).await?;
    base::tokens(desktop, 4).await?;
    base::query(desktop, "@alex").await?;
    mixed_ready(state, desktop).await?;
    base::ledger(state, 0, 2).await?;
    let before = base::dom(desktop, "state", Value::Null).await?;
    {
        let mut shell = state.write();
        let team = shell.core.active_team.clone().ok_or("Missing team")?;
        shell
            .core
            .workspaces
            .get_mut(&team)
            .unwrap()
            .usergroups
            .get_mut(groups::FIRST)
            .ok_or("Missing native group")?
            .handle = "untracked-drift".into();
    }
    choose(state, desktop, groups::FIRST, false).await?;
    let previous = before["sequence"].as_u64().ok_or("Missing sequence")?;
    let js = base::JS;
    let after=helper::wait_dom(desktop,&format!("(()=>{{const v=({js})('state',null);v.ok=v.sequence>{previous}&&v.native?.tokens?.length===4;return v;}})()")).await?;
    if after["native"]["snapshot"]["blocks"] != before["native"]["snapshot"]["blocks"] {
        return Err("Stale group inserted".into());
    }
    capture(state,desktop,name,"stale-refused",None,json!({"refused":true,"previousSequence":previous,"nextSequence":after["sequence"],"exactNativeBlocks":after["native"]["snapshot"]["blocks"]}),&mut stages,&mut captures).await?;
    Ok(
        json!({"ok":true,"fixture":name,"restricted":false,"readiness":readiness,"stages":stages,"captures":captures,"initialBlocks":initial(),"savedBlocks":expected,"source":source::snapshot(&state.peek()),"model":mentions::snapshot(&state.peek()),"limitations":["Synthetic requests only; no HTTP.","Native DOM keyboard/mouse events, not OS IME.","Popup screenshots are scrolled samples; all seven results covered.","Global75/25 cap is covered by focused core tests; native uses seven rows."]}),
    )
}
