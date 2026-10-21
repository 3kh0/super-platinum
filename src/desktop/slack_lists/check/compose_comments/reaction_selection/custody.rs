use super::*;
const STAGES: &[&str] = &[
    "draft",
    "catalog-bound",
    "catalog-stale",
    "prefs-bound",
    "prefs-stale",
    "preflight-held",
    "preflight-retired",
];
fn expected(i: usize) -> Expected {
    Expected {
        reads: if i >= 5 { 8 } else { 4 },
        posts: 0,
        completed: 0,
        phases: match i {
            5 => vec!["preparing"],
            6 => vec!["not-sent"],
            _ => vec![],
        },
        picker: match i {
            1 | 3 => Some(true),
            2 | 4 => Some(false),
            _ => None,
        },
        reactions: data::initial_reactions(),
    }
}
async fn stage(
    s: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    i: usize,
    p: &Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    capture(s, m, d, STAGES[i], p, rows, expected(i)).await
}
fn prefs(
    mut state: Signal<ShellState>,
    m: &Model,
    p: &mut Value,
    stage: &str,
    value: i32,
) -> Result<(), String> {
    let frame = json!({"type":"pref_change","name":"preferred_skin_tone","value":value});
    let event = super_platinum_core::slack::realtime::parse_event(&frame.to_string())
        .ok_or("Preference event missing")?;
    let before = data::observed(&state.peek(), m)?;
    crate::realtime::apply_in_view(&mut state.write().core, "T1", 7, event, false);
    let after = data::observed(&state.peek(), m)?;
    if before["preferences"] == after["preferences"] {
        return Err("Actual preference event was not applied".into());
    }
    p["sourceChanges"].as_array_mut().unwrap().push(json!({"stage":stage,"kind":"pref_change","frame":frame,"team":"T1","generation":7,"before":before,"after":after}));
    Ok(())
}
async fn refused(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    p: &mut Value,
    stage: &str,
) -> Result<(), String> {
    wait(state, m, |v| v["picker"]["ready"] == false).await?;
    let before = snapshot(state, m)?;
    let input = pick(d, "+1", true).await?;
    settled(d).await?;
    let after = snapshot(state, m)?;
    if before != after {
        return Err("Disabled stale picker choice mutated original state".into());
    }
    p["inputs"]
        .as_array_mut()
        .unwrap()
        .push(json!({"stage":stage,"value":input,"unchanged":true}));
    Ok(())
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    p: &mut Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    stage(state, m, d, 0, p, rows).await?;
    open_picker(d).await?;
    search(d, "+1").await?;
    stage(state, m, d, 1, p, rows).await?;
    let before = data::observed(&state.peek(), m)?;
    {
        let mut s = state.write();
        let w = s.core.workspaces.get_mut("T1").ok_or("Workspace missing")?;
        w.custom_emoji
            .get_mut(data::ALIAS)
            .ok_or("Original custom alias missing")?
            .value = "alias:eyes".into();
        w.custom_emoji_revision += 1;
    }
    let after = data::observed(&state.peek(), m)?;
    p["sourceChanges"].as_array_mut().unwrap().push(json!({"stage":"catalog-stale","kind":"accepted-custom-metadata","name":data::ALIAS,"value":"alias:eyes","before":before,"after":after}));
    // Let the actual shared catalog builder publish the new immutable allocation.
    crate::emoji_catalog::ensure(state);
    wait(state, m, |v| !v["selectionSource"]["catalog"].is_null()).await?;
    refused(state, m, d, p, "catalog-stale").await?;
    stage(state, m, d, 2, p, rows).await?;
    close_picker(d).await?;
    open_picker(d).await?;
    search(d, "+1").await?;
    stage(state, m, d, 3, p, rows).await?;
    prefs(state, m, p, "prefs-stale", 5)?;
    refused(state, m, d, p, "prefs-stale").await?;
    stage(state, m, d, 4, p, rows).await?;
    close_picker(d).await?;
    compose::mode(m, compose::Mode::HeldPreflight)?;
    choose(
        state,
        m,
        d,
        p,
        "preflight-held",
        "eyes",
        dispatch::Mode::Success,
    )
    .await?;
    wait(state, m, |v| {
        v["composeLedger"]["preflights"].as_array().map(Vec::len) == Some(1)
    })
    .await?;
    stage(state, m, d, 5, p, rows).await?;
    prefs(state, m, p, "preflight-retired", 6)?;
    let release = compose::release(m, "T1")?;
    if release != json!({"delivered":true,"cancelledBeforeReply":false}) {
        return Err("Original preflight receipt was not delivered".into());
    }
    wait(state, m, |v| {
        v["reactionActions"]["attempts"]
            .as_array()
            .is_some_and(|a| a.last().is_some_and(|a| a["phase"] == "not-sent"))
    })
    .await?;
    p["releases"]
        .as_array_mut()
        .unwrap()
        .push(json!({"stage":"preflight-retired","value":release}));
    stage(state, m, d, 6, p, rows).await
}
