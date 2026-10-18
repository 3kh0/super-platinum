//! Actual @ completion through the native editor, private catalog and section
//! save/readback. Shared channel checker utilities own DOM input and captures.
use super::*;
use super::{completion as source, completion_check as base, mentions, mentions_check as helper};
use dioxus::desktop::DesktopContext;
use serde_json::json;
use std::time::Duration;
pub(crate) const FLAG: &str = "SUPER_PLATINUM_PROFILE_MEMBER_COMPLETION_CHECK";
const PREFIX: &str = "profile-member-completion-check:";
const CHOOSE: &str = include_str!("member_completion_check.js");
const LOCAL: &str = "@Alex Local";
const SAME: &str = "@Alex Harbor";
const FAILURE: &str = "People suggestions could not load. Edit your query to try again.";

pub(crate) fn start(state: Signal<ShellState>, desktop: DesktopContext) {
    if std::env::var(source::groups::FLAG).as_deref() == Ok("1")
        || !source::named()
        || std::env::var(FLAG).as_deref() != Ok("1")
        || std::env::var(source::FLAG).as_deref() != Ok("1")
    {
        return;
    }
    let name = std::env::var("SUPER_PLATINUM_FIXTURE").unwrap();
    dioxus::core::spawn_forever(async move {
        let result = match tokio::time::timeout(
            Duration::from_secs(160),
            run(state, &desktop, &name),
        )
        .await
        {
            Ok(Ok(value)) => value,
            Ok(Err(error)) => json!({"ok":false,"fixture":name,"error":error,
                "model":mentions::snapshot(&state.peek()),"source":source::snapshot(&state.peek())}),
            Err(_) => json!({"ok":false,"fixture":name,"error":"Member completion check timed out",
                "source":source::snapshot(&state.peek())}),
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
async fn choose(
    desktop: &DesktopContext,
    occurrence: usize,
    keyboard: bool,
) -> Result<Value, String> {
    helper::wait_dom(
        desktop,
        &format!(
            "({CHOOSE})({})",
            json!({"label":SAME,"occurrence":occurrence,"keyboard":keyboard})
        ),
    )
    .await
}
async fn pair(desktop: &DesktopContext) -> Result<Value, String> {
    let js = base::JS;
    helper::wait_dom(desktop,&format!(r#"(()=>{{const v=({js})('state',null);v.ok=v.rows.filter(r=>r.label==={}).length===2;return v;}})()"#,json!(SAME))).await
}
async fn capture(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    stage: &str,
    proof: Value,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    let index = match stage {
        "refreshed" => Some(0),
        "refreshed-middle" => Some(2),
        "refreshed-bottom" => Some(4),
        _ => None,
    };
    if let Some(index) = index {
        helper::wait_dom(
            desktop,
            &format!("({CHOOSE})({})", json!({"action":"scroll","index":index})),
        )
        .await?;
    }
    let js = base::JS;
    let geometry = helper::wait_dom(
        desktop,
        &format!(
            "({CHOOSE})({{action:'layout',index:{},geometry:({js})('layout',null)}})",
            json!(index)
        ),
    )
    .await
    .map_err(|e| format!("Member layout {stage}: {e}"))?;
    base::capture_measured(
        state, desktop, name, stage, proof, stages, captures, geometry,
    )
    .await
}
fn check_inserted(native: &Value, expected: &[&str]) -> Result<(), String> {
    let tokens = native["native"]["tokens"]
        .as_array()
        .ok_or("Missing resolved token custody")?;
    let ids = tokens
        .iter()
        .skip(2)
        .map(|t| {
            t["value"]["user_id"]
                .as_str()
                .ok_or("Missing inserted member identity")
        })
        .collect::<Result<Vec<_>, _>>()?;
    if ids != expected {
        return Err(format!(
            "Distinct same-label member selection mismatch: {ids:?}"
        ));
    }
    Ok(())
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    let readiness = helper::readiness(name).await?;
    if !source::enabled(&state.peek()) || !mentions::user_mode() {
        return Err("Closed member fixture unavailable".into());
    }
    mentions::seed(state, "Alex Local").await?;
    mentions::set_blocks(&mut state.write(), initial());
    helper::select_values(state, desktop).await?;
    helper::click(desktop, "Edit description", false).await?;
    base::tokens(desktop, 2).await?;
    let mut stages = Vec::new();
    let mut captures = serde_json::Map::new();
    capture(
        state,
        desktop,
        name,
        "opened",
        json!({"initialBlocks":initial()}),
        &mut stages,
        &mut captures,
    )
    .await?;
    base::mode(state, source::Mode::Held)?;
    base::query(desktop, "@alex").await?;
    base::ledger(state, 1, 0).await?;
    let local = base::rows(desktop, &[LOCAL], true).await?;
    capture(
        state,
        desktop,
        name,
        "local",
        json!({"sequence":local["sequence"],"held":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    let release = source::release(&state.peek()).map_err(|e| e.to_string())?;
    if release["delivered"] != true {
        return Err("Held member response did not deliver".into());
    }
    base::ledger(state, 0, 0).await?;
    let refreshed = pair(desktop).await?;
    if local["sequence"] != refreshed["sequence"] {
        return Err("Remote member refresh replaced query".into());
    }
    base::rows(desktop, &[LOCAL, "@Alex Retired", "@Alex Self"], false).await?;
    let accepted_members = member_rows(&state.peek())?;
    for stage in ["refreshed", "refreshed-middle", "refreshed-bottom"] {
        capture(state,desktop,name,stage,json!({"sequence":refreshed["sequence"],"release":release,"acceptedMembers":accepted_members}),&mut stages,&mut captures).await?;
    }
    let chosen = choose(desktop, 0, true).await?;
    let inserted = base::tokens(desktop, 3).await?;
    check_inserted(&inserted, &[source::members::FIRST])?;
    capture(
        state,
        desktop,
        name,
        "keyboard",
        json!({"user":source::members::FIRST,"keyboard":true,"choice":chosen}),
        &mut stages,
        &mut captures,
    )
    .await?;
    base::query(desktop, "@harbor").await?;
    pair(desktop).await?;
    base::ledger(state, 0, 0).await?;
    let chosen = choose(desktop, 1, false).await?;
    let inserted = base::tokens(desktop, 4).await?;
    check_inserted(
        &inserted,
        &[source::members::FIRST, source::members::SECOND],
    )?;
    let expected = inserted["native"]["snapshot"]["blocks"].clone();
    capture(state,desktop,name,"mouse",json!({"user":source::members::SECOND,"keyboard":false,"choice":chosen,"expectedBlocks":expected}),&mut stages,&mut captures).await?;
    helper::click(desktop, "Use description", false).await?;
    helper::wait_dom(desktop,r#"({ok:!document.getElementById('profile-section-rich')&&!!document.getElementById('profile-section-field-long')})"#).await?;
    if mentions::snapshot(&state.peek())["blocks"] != expected {
        return Err("Use description changed member blocks".into());
    }
    helper::save(state, desktop, Outcome::Saved, 1, 1).await?;
    helper::click(desktop, "Edit description", false).await?;
    let reopened = base::tokens(desktop, 4).await?;
    if reopened["native"]["snapshot"]["blocks"] != expected {
        return Err("Member Save/readback changed native blocks".into());
    }
    capture(
        state,
        desktop,
        name,
        "saved",
        json!({"exactReadback":true,"expectedBlocks":expected}),
        &mut stages,
        &mut captures,
    )
    .await?;
    base::mode(state, source::Mode::Held)?;
    base::query(desktop, "@alex-held").await?;
    base::ledger(state, 1, 0).await?;
    base::query(desktop, "@harbor").await?;
    pair(desktop).await?;
    base::ledger(state, 0, 1).await?;
    let release = source::release(&state.peek()).map_err(|e| e.to_string())?;
    if release["cancelledBeforeReply"] != true || release["delivered"] != false {
        return Err("Old member query kept transport authority".into());
    }
    capture(
        state,
        desktop,
        name,
        "query-retired",
        json!({"release":release,"tokensUnchanged":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    for (stage, term, mode) in [
        ("malformed", "@no-malformed", source::Mode::Malformed),
        ("failure", "@no-offline", source::Mode::Failure),
    ] {
        base::mode(state, mode)?;
        base::query(desktop, term).await?;
        base::status(desktop, FAILURE).await?;
        base::ledger(state, 0, 1).await?;
        capture(
            state,
            desktop,
            name,
            stage,
            json!({"retainedBlocks":expected,"noWrite":true}),
            &mut stages,
            &mut captures,
        )
        .await?;
    }
    base::mode(state, source::Mode::Empty)?;
    base::query(desktop, "@zz-no-matching-person").await?;
    base::status(desktop, "No matching people").await?;
    base::ledger(state, 0, 1).await?;
    capture(
        state,
        desktop,
        name,
        "empty",
        json!({"empty":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    base::mode(state, source::Mode::Held)?;
    base::query(desktop, "@alex-cancel").await?;
    base::ledger(state, 1, 1).await?;
    helper::click(desktop, "Cancel description edit", false).await?;
    helper::wait_dom(
        desktop,
        r#"({ok:!document.getElementById('profile-section-rich')})"#,
    )
    .await?;
    base::ledger(state, 0, 2).await?;
    let release = source::release(&state.peek()).map_err(|e| e.to_string())?;
    if release["cancelledBeforeReply"] != true
        || release["delivered"] != false
        || mentions::snapshot(&state.peek())["blocks"] != expected
    {
        return Err("Member cancel changed saved draft or accepted late reply".into());
    }
    capture(
        state,
        desktop,
        name,
        "cancelled",
        json!({"release":release,"expectedBlocks":expected}),
        &mut stages,
        &mut captures,
    )
    .await?;
    helper::click(desktop, "Edit description", false).await?;
    base::tokens(desktop, 4).await?;
    base::query(desktop, "@harbor").await?;
    let before = pair(desktop).await?;
    base::ledger(state, 0, 2).await?;
    {
        let mut shell = state.write();
        let team = shell.core.active_team.clone().ok_or("Missing team")?;
        shell
            .core
            .workspaces
            .get_mut(&team)
            .unwrap()
            .directory_members
            .invalidate(source::members::FIRST);
    }
    choose(desktop, 0, false).await?;
    let previous = before["sequence"]
        .as_u64()
        .ok_or("Missing original sequence")?;
    let js = base::JS;
    let proof=helper::wait_dom(desktop,&format!(r#"(()=>{{const v=({js})('state',null);v.ok=v.sequence>{previous}&&v.native?.tokens?.length===4;return v;}})()"#)).await?;
    if proof["native"]["snapshot"]["blocks"] != before["native"]["snapshot"]["blocks"] {
        return Err("Stale member choice altered native draft".into());
    }
    capture(state,desktop,name,"stale-refused",json!({"previousSequence":previous,"nextSequence":proof["sequence"],"exactNativeBlocks":proof["native"]["snapshot"]["blocks"],"refused":true}),&mut stages,&mut captures).await?;
    Ok(
        json!({"ok":true,"fixture":name,"readiness":readiness,"stages":stages,"captures":captures,
        "initialBlocks":initial(),"savedBlocks":expected,"source":source::snapshot(&state.peek()),"model":mentions::snapshot(&state.peek()),
        "limitations":["Closed synthetic transport only; no Slack calls.","Programmatic native DOM keyboard/mouse input; no OS IME claim.","Popup-stage captures are scrolled samples; original token custody is retained separately."]}),
    )
}
fn member_rows(shell: &ShellState) -> Result<Value, String> {
    let team = shell
        .core
        .active_team
        .as_deref()
        .ok_or("Missing fixture team")?;
    let ctx = crate::directory_member_fetches::Context::capture(shell, team)
        .map_err(|e| e.to_string())?;
    let w = &shell.core.workspaces[team];
    let ready = w
        .directory_members
        .ready(
            &ctx.account,
            &ctx.session,
            ctx.epoch,
            "member-completion-proof",
            &w.users,
        )
        .map_err(|e| e.to_string())?;
    Ok(json!(
        ready.rows().iter().map(|r| &r.source).collect::<Vec<_>>()
    ))
}
