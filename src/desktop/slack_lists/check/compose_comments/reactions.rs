//! Parsed socket frames through production List ingress, with a mounted rich draft.
use super::*;
const FIRST: &str = "1781524801.000001";
const CHANNEL: &str = "CCOMMENT01";
const STAGES: &[&str] = &[
    "draft",
    "added",
    "duplicate",
    "removed",
    "foreign",
    "sent",
    "tail-added",
    "tail-removed",
];

fn inject(
    mut state: Signal<ShellState>,
    stage: &str,
    team: &str,
    generation: u64,
    channel: &str,
    ts: &str,
    added: bool,
    expected: bool,
) -> Result<Value, String> {
    let frame = json!({"type":if added {"reaction_added"}else{"reaction_removed"},
        "user":"U_RT_REACTOR","reaction":"wave","item":{"type":"message","channel":channel,"ts":ts}});
    let event = super_platinum_core::slack::realtime::parse_event(&frame.to_string())
        .ok_or("Reaction frame not parsed")?;
    let mut shell = state.write();
    if !compose::enabled(&shell) || shell.core.session.is_some() || shell.core.transport.is_some() {
        return Err("Disconnected reaction fixture required".into());
    }
    let before = crate::slack_lists::comments::presentation(&shell)
        .ok_or("Comments missing")?
        .rows;
    if crate::slack_lists::events::prepare(&mut shell, team, generation, &event) {
        return Err("Reaction incorrectly requested a List refresh".into());
    }
    let after = crate::slack_lists::comments::presentation(&shell)
        .ok_or("Comments lost")?
        .rows;
    let changed = !std::sync::Arc::ptr_eq(&before, &after);
    if changed != expected {
        return Err(format!("Unexpected reaction change at {stage}"));
    }
    Ok(json!({"stage":stage,"team":team,"generation":generation,"frame":frame,"changed":changed}))
}

async fn capture(
    state: Signal<ShellState>,
    model: &Model,
    d: &dioxus::desktop::DesktopContext,
    name: &str,
    ts: &str,
    expected: u64,
    proof: &Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    if STAGES.get(rows.len()) != Some(&name) {
        return Err("Reaction stage order".into());
    }
    stable_drafts(state, model).await?;
    let expression = format!(
        "(()=>{{const v=window.__listCompose.snapshot(),r=v.rows.find(r=>r.ts==={});v.ok=v.ok&&!!r&&(r.reactions.find(p=>p.name==='wave')?.count||0)==={expected}&&v.editor?.editable===true;return v;}})()",
        json!(ts)
    );
    wait_dom(d, &expression).await?;
    // Explicit capture-only scroll exposes a narrow pill without moving editor focus.
    checked(d,&format!("(()=>{{const r=[...document.querySelectorAll('[data-list-comment-ts]')].find(r=>r.dataset.listCommentTs==={});if(!r)return {{ok:false}};(r.querySelector('[data-reaction-name=wave]')||r).scrollIntoView({{block:'nearest'}});return {{ok:true}};}})()",json!(ts))).await?;
    let geometry = wait_dom(d, &expression).await?;
    let before = compose::snapshot(&state.peek(), model);
    if before["noSession"] != true
        || before["noTransport"] != true
        || before["writes"] != 0
        || before["networkCalls"] != 0
    {
        return Err("Reaction fixture acquired live state".into());
    }
    let mut proof = proof.clone();
    proof["journal"] = compose::journal_snapshot(state).await?;
    let view =
        crate::slack_lists::comments::presentation(&state.peek()).ok_or("Comments missing")?;
    let message = view
        .rows
        .iter()
        .find(|m| m.ts == ts)
        .ok_or("Reaction row missing")?;
    let count = message
        .reactions
        .iter()
        .find(|r| r.name == "wave")
        .map_or(0, |r| r.count);
    if u64::from(count) != expected {
        return Err("Model reaction count changed".into());
    }
    let probe = json!({"ts":ts,"count":count,"channel":view.channel,"editorGeneration":geometry["editor"]["generation"],
        "hasMore":view.has_more,"readRequests":before["ledger"]["requests"].as_array().unwrap().len(),
        "prepared":before["composeLedger"]["prepared"].as_array().unwrap().len(),"posts":before["composeLedger"]["posts"].as_array().unwrap().len()});
    let capture = harness::capture(d, name).await?;
    if compose::snapshot(&state.peek(), model) != before
        || compose::journal_snapshot(state).await? != proof["journal"]
    {
        return Err(format!("Reaction state changed during {name} capture"));
    }
    rows.push(json!({"stage":name,"geometry":geometry,"model":before,"proof":proof,"reactionProbe":probe,"capture":capture}));
    Ok(())
}

pub(super) async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let readiness = harness::wait_runner(state).await?;
    let model = fixture::begin(&mut state.write())?;
    state
        .write()
        .core
        .workspaces
        .get_mut("T1")
        .ok_or("Fixture workspace missing")?
        .rt_generation = 7;
    checked(d, include_str!("../comments.js")).await?;
    checked(d, include_str!("../compose_comments.js")).await?;
    wait_dom(
        d,
        "({ok:document.querySelector('.slack-list-dialog')?.dataset.listKeyboardMounted==='true'})",
    )
    .await?;
    open(d, "R_LIST_000").await?;
    let seed = compose::seed_draft(state).await?;
    write(d).await?;
    type_text(
        d,
        "Final review \u{4e2d}\u{1f642}\nSecond line <literal> & *punctuation*.",
    )
    .await?;
    checked(d, "window.__listCompose.bold()").await?;
    settled(d).await?;
    let native=wait_dom(d,"(()=>{const v=window.__listCompose.snapshot();v.ok=v.ok&&v.editor?.bold===true;return v;})()").await?;
    let blocks = native["editor"]["blocks"].clone();
    stable_drafts(state, &model).await?;
    if compose::journal_snapshot(state).await?["blocks"] != blocks {
        return Err("Rich native draft not saved".into());
    }
    checked(d,"(()=>{window.__listReactionOriginalEditor=document.getElementById('slack-list-comment-draft');return {ok:!!window.__listReactionOriginalEditor};})()").await?;
    let proof = json!({"seed":seed,"blocks":blocks});
    let mut rows = Vec::new();
    let mut events = Vec::new();
    capture(state, &model, d, "draft", FIRST, 2, &proof, &mut rows).await?;
    for (stage, added, changed, count) in [
        ("added", true, true, 3),
        ("duplicate", true, false, 3),
        ("removed", false, true, 2),
    ] {
        events.push(inject(
            state, stage, "T1", 7, CHANNEL, FIRST, added, changed,
        )?);
        capture(state, &model, d, stage, FIRST, count, &proof, &mut rows).await?;
    }
    for (team, generation, channel, ts) in [
        ("T1", 7, "COTHER", FIRST),
        ("T_OTHER", 7, CHANNEL, FIRST),
        ("T1", 6, CHANNEL, FIRST),
        ("T1", 7, CHANNEL, "1999999999.000001"),
    ] {
        events.push(inject(
            state, "foreign", team, generation, channel, ts, true, false,
        )?);
    }
    capture(state, &model, d, "foreign", FIRST, 2, &proof, &mut rows).await?;
    compose::mode(&model, Mode::Success)?;
    click(d, "slack-list-comment-send").await?;
    let accepted = wait_model(state, &model, |v| {
        counts(v, 1, 1)
            && v["draftStore"]["drafts"].as_array().is_some_and(|rows| {
                rows.len() == 1
                    && rows[0]["attempt"].is_null()
                    && rows[0]["blocks"].as_array().is_some_and(Vec::is_empty)
            })
    })
    .await?;
    let sent = accepted["composeLedger"]["messages"][0]["ts"]
        .as_str()
        .ok_or("Accepted TS missing")?
        .to_owned();
    capture(state, &model, d, "sent", &sent, 0, &proof, &mut rows).await?;
    for (stage, added, count) in [("tail-added", true, 1), ("tail-removed", false, 0)] {
        events.push(inject(state, stage, "T1", 7, CHANNEL, &sent, added, true)?);
        capture(state, &model, d, stage, &sent, count, &proof, &mut rows).await?;
    }
    Ok(
        json!({"ok":true,"case":"reactions","fixture":readiness["fixture"],"readiness":readiness,
        "stages":rows,"reactionEvents":events,"final":compose::snapshot(&state.peek(),&model),
        "initialRecords":fixture::initial_records(&model),"noLiveNetwork":true}),
    )
}
