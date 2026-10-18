//! Real native editor/caret -> private completion catalog -> section wire.
//! All transport outcomes come from the separately gated synthetic producer.
use super::*;
use super::{completion as source, mentions, mentions_check as helper};
use dioxus::desktop::DesktopContext;
use serde_json::json;
use std::time::Duration;
pub(super) const JS: &str = include_str!("completion_check.js");
const PREFIX: &str = "profile-channel-completion-check:";
const EDGE: &str = "profile-channel-completion-capture:";

pub(crate) fn start(state: Signal<ShellState>, desktop: DesktopContext) {
    if !source::named()
        || std::env::var(source::FLAG).as_deref() != Ok("1")
        || std::env::var("SUPER_PLATINUM_PROFILE_MEMBER_COMPLETION_CHECK").as_deref() == Ok("1")
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
            Ok(Err(error)) => {
                json!({"ok":false,"fixture":name,"error":error,"model":mentions::snapshot(&state.peek()),"source":source::snapshot(&state.peek())})
            }
            Err(_) => {
                json!({"ok":false,"fixture":name,"error":"Profile completion check timed out","source":source::snapshot(&state.peek())})
            }
        };
        eprintln!("{PREFIX} {result}");
    });
}
pub(super) fn initial() -> Vec<Value> {
    let mut blocks = mentions::blocks("Working with ");
    blocks[0]["elements"][0]["elements"][0]["style"] = json!({"bold":true});
    blocks[0]["elements"][0]["elements"][1]["style"] = json!({"italic":true});
    blocks
}
async fn run(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    let readiness = helper::readiness(name).await?;
    if !source::enabled(&state.peek()) {
        return Err("Closed completion fixture unavailable".into());
    }
    mentions::seed(state, mentions::LABEL).await?;
    mentions::set_blocks(&mut state.write(), initial());
    helper::select_values(state, desktop).await?;
    helper::click(desktop, "Edit description", false).await?;
    tokens(desktop, 2).await?;
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

    mode(state, source::Mode::Held)?;
    query(desktop, "#project").await?;
    ledger(state, 1, 0).await?;
    let local = rows(desktop, &["#project-harbor"], true).await?;
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
    let released = source::release(&state.peek()).map_err(|e| e.to_string())?;
    if released["delivered"] != true {
        return Err("Initial held reply did not deliver".into());
    }
    ledger(state, 0, 0).await?;
    let refreshed = rows(
        desktop,
        &["#project-harbor", "#project-completion", "#project-cove"],
        true,
    )
    .await?;
    if local["sequence"] != refreshed["sequence"] {
        return Err("Remote result replaced native query identity".into());
    }
    capture(
        state,
        desktop,
        name,
        "refreshed",
        json!({"sequence":refreshed["sequence"],"release":released}),
        &mut stages,
        &mut captures,
    )
    .await?;
    dom(
        desktop,
        "choose",
        json!({"label":"#project-completion","keyboard":true}),
    )
    .await?;
    tokens(desktop, 3).await?;
    capture(
        state,
        desktop,
        name,
        "keyboard",
        json!({"channel":source::REMOTE,"keyboard":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    query(desktop, "#project-co").await?;
    rows(desktop, &["#project-completion", "#project-cove"], false).await?;
    dom(
        desktop,
        "choose",
        json!({"label":"#project-cove","keyboard":false}),
    )
    .await?;
    let inserted = tokens(desktop, 4).await?;
    let expected = inserted["native"]["snapshot"]["blocks"].clone();
    capture(
        state,
        desktop,
        name,
        "mouse",
        json!({"channel":source::SECOND,"keyboard":false,"expectedBlocks":expected}),
        &mut stages,
        &mut captures,
    )
    .await?;
    helper::click(desktop, "Use description", false).await?;
    helper::wait_dom(desktop,r#"({ok:!document.getElementById('profile-section-rich')&&!!document.getElementById('profile-section-field-long')})"#).await?;
    if mentions::snapshot(&state.peek())["blocks"] != expected {
        return Err("Use description lost exact native completion blocks".into());
    }
    helper::save(state, desktop, Outcome::Saved, 1, 1).await?;
    helper::click(desktop, "Edit description", false).await?;
    let reopened = tokens(desktop, 4).await?;
    if reopened["native"]["snapshot"]["blocks"] != expected {
        return Err("Saved completion readback changed native blocks".into());
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

    mode(state, source::Mode::Held)?;
    query(desktop, "#project-held").await?;
    ledger(state, 1, 0).await?;
    query(desktop, "#project-co").await?;
    rows(desktop, &["#project-completion", "#project-cove"], false).await?;
    ledger(state, 0, 1).await?;
    let retired = source::release(&state.peek()).map_err(|e| e.to_string())?;
    if retired["cancelledBeforeReply"] != true {
        return Err("Replaced native query retained request authority".into());
    }
    capture(
        state,
        desktop,
        name,
        "query-retired",
        json!({"release":retired,"tokensUnchanged":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    for (stage, term, value) in [
        ("malformed", "#no-malformed", source::Mode::Malformed),
        ("failure", "#no-offline", source::Mode::Failure),
    ] {
        mode(state, value)?;
        query(desktop, term).await?;
        status(
            desktop,
            "Channel suggestions could not load. Edit your query to try again.",
        )
        .await?;
        ledger(state, 0, 1).await?;
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
    mode(state, source::Mode::Empty)?;
    query(desktop, "#zz-no-matching-channel").await?;
    status(desktop, "No matching channels").await?;
    ledger(state, 0, 1).await?;
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
    mode(state, source::Mode::Held)?;
    query(desktop, "#project-cancel").await?;
    ledger(state, 1, 1).await?;
    helper::click(desktop, "Cancel description edit", false).await?;
    helper::wait_dom(
        desktop,
        r#"({ok:!document.getElementById('profile-section-rich')})"#,
    )
    .await?;
    ledger(state, 0, 2).await?;
    let cancelled = source::release(&state.peek()).map_err(|e| e.to_string())?;
    if cancelled["cancelledBeforeReply"] != true
        || mentions::snapshot(&state.peek())["blocks"] != expected
    {
        return Err("Editor cancellation lost saved draft or request custody".into());
    }
    capture(
        state,
        desktop,
        name,
        "cancelled",
        json!({"release":cancelled,"expectedBlocks":expected}),
        &mut stages,
        &mut captures,
    )
    .await?;

    helper::click(desktop, "Edit description", false).await?;
    tokens(desktop, 4).await?;
    query(desktop, "#project").await?;
    let before = rows(
        desktop,
        &["#project-harbor", "#project-completion", "#project-cove"],
        true,
    )
    .await?;
    ledger(state, 0, 2).await?;
    {
        let mut s = state.write();
        let team = s.core.active_team.clone().ok_or("Missing fixture team")?;
        s.core
            .workspaces
            .get_mut(&team)
            .unwrap()
            .directory_channels
            .invalidate(mentions::CHANNEL);
    }
    dom(
        desktop,
        "choose",
        json!({"label":"#project-harbor","keyboard":false}),
    )
    .await?;
    let old_sequence = before["sequence"]
        .as_u64()
        .ok_or("Missing original popup sequence")?;
    let proof=helper::wait_dom(desktop,&format!(r#"(()=>{{const v=({JS})('state',null);v.ok=v.sequence>{old_sequence}&&v.native?.tokens?.length===4;return v;}})()"#)).await?;
    if proof["native"]["snapshot"]["blocks"] != before["native"]["snapshot"]["blocks"] {
        return Err("Stale metadata choice changed native description".into());
    }
    capture(state,desktop,name,"stale-refused",json!({"previousSequence":old_sequence,"nextSequence":proof["sequence"],"exactNativeBlocks":proof["native"]["snapshot"]["blocks"],"refused":true}),&mut stages,&mut captures).await?;
    Ok(
        json!({"ok":true,"fixture":name,"readiness":readiness,"stages":stages,"captures":captures,"initialBlocks":initial(),"savedBlocks":expected,"source":source::snapshot(&state.peek()),"model":mentions::snapshot(&state.peek()),
        "limitations":["Synthetic held transport only; no Slack requests or browser/network probes.","Programmatic native DOM input, selection and keyboard/mouse events; no OS IME claim.","Result-stage screenshots prioritize the popup and action controls; original token geometry is separately retained."]}),
    )
}
pub(super) fn mode(state: Signal<ShellState>, value: source::Mode) -> Result<(), String> {
    source::mode(&state.peek(), value).map_err(|e| e.to_string())
}
pub(super) async fn ledger(
    state: Signal<ShellState>,
    active: u64,
    cancelled: u64,
) -> Result<(), String> {
    for _ in 0..80 {
        let v = source::snapshot(&state.peek());
        if v["active"] == active && v["cancelled"].as_u64().is_some_and(|v| v >= cancelled) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!(
        "Completion ledger active {active} cancelled {cancelled}: {}",
        source::snapshot(&state.peek())
    ))
}
pub(super) async fn dom(
    desktop: &DesktopContext,
    action: &str,
    arg: Value,
) -> Result<Value, String> {
    helper::wait_dom(desktop, &format!("({JS})({}, {arg})", json!(action))).await
}
pub(super) async fn query(desktop: &DesktopContext, term: &str) -> Result<(), String> {
    // A previous query's error/rows may remain painted when native input fires.
    // Do not let a later status check accept them before this input is admitted.
    let input = dom(desktop, "query", json!(term)).await?;
    dom(desktop, "query-ready", input).await.map(|_| ())
}
pub(super) async fn tokens(desktop: &DesktopContext, count: usize) -> Result<Value, String> {
    helper::wait_dom(desktop,&format!(r#"(()=>{{const v=({JS})('state',null);v.ok=v.native?.tokens?.length==={count};return v;}})()"#)).await
}
pub(super) async fn rows(
    desktop: &DesktopContext,
    labels: &[&str],
    exact: bool,
) -> Result<Value, String> {
    helper::wait_dom(desktop,&format!(r#"(()=>{{const v=({JS})('state',null),expected={};v.ok=expected.every(s=>v.rows.some(r=>r.label===s))&&(!{exact}||v.rows.length===expected.length);return v;}})()"#,json!(labels))).await
}
pub(super) async fn status(desktop: &DesktopContext, message: &str) -> Result<(), String> {
    helper::wait_dom(
        desktop,
        &format!(
            r#"(()=>{{const v=({JS})('state',null);v.ok=v.status==={};return v;}})()"#,
            json!(message)
        ),
    )
    .await
    .map(|_| ())
}
pub(super) async fn capture(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    stage: &str,
    proof: Value,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    let geometry = dom(desktop, "layout", Value::Null)
        .await
        .map_err(|e| format!("Layout {stage}: {e}"))?;
    capture_measured(
        state, desktop, name, stage, proof, stages, captures, geometry,
    )
    .await
}

pub(super) async fn capture_measured(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    stage: &str,
    proof: Value,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
    mut geometry: Value,
) -> Result<(), String> {
    let directory = std::env::var_os("SUPER_PLATINUM_UI_CAPTURE_DIR")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or("Absolute capture directory required")?;
    let path = directory.join(format!("{name}-{stage}.png"));
    let inspect = path.clone();
    if tokio::task::spawn_blocking(move || inspect.try_exists())
        .await
        .map_err(|_| "Capture check worker")?
        .map_err(|_| "Capture path check")?
    {
        return Err("Capture must be fresh".into());
    }
    eprintln!("{EDGE} {}", json!({"stage":stage,"edge":"before"}));
    let (path, width, height) =
        crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), desktop).await?;
    geometry["pngDimensions"] = json!([width, height]);
    captures.insert(stage.into(), json!(path.to_string_lossy()));
    stages.push(json!({"stage":stage,"proof":proof,"geometry":geometry,"model":mentions::snapshot(&state.peek()),"source":source::snapshot(&state.peek())}));
    eprintln!("{EDGE} {}", json!({"stage":stage,"edge":"after"}));
    Ok(())
}
