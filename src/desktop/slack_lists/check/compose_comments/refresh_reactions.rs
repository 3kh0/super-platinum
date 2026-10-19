//! Held original reads cannot replace newer parsed socket deltas or mounted drafts.
use super::*;

const FIRST: &str = "1781524801.000001";
const CHANNEL: &str = "CCOMMENT01";
const NOTICE: &str =
    "Newer comment updates arrived. Loaded comments are retained. Refresh to try again.";
const STAGES: &[&str] = &[
    "draft",
    "held",
    "ignored-held",
    "updated",
    "released",
    "refreshed",
    "sent",
    "tail-draft",
    "tail-held",
    "tail-updated",
    "tail-released",
    "tail-refreshed",
];
const READS: &[u64] = &[4, 8, 8, 8, 8, 12, 16, 16, 20, 20, 20, 24];
const COMPLETED: &[u64] = &[4, 7, 7, 8, 8, 12, 16, 16, 19, 20, 20, 24];
const CANCELLED: &[u64] = &[0, 0, 0, 1, 1, 1, 1, 1, 1, 2, 2, 2];

fn inject(
    mut state: Signal<ShellState>,
    stage: &str,
    team: &str,
    generation: u64,
    channel: &str,
    ts: &str,
    user: &str,
    expected: bool,
) -> Result<Value, String> {
    let frame = json!({"type":"reaction_added","user":user,"reaction":"wave",
        "item":{"type":"message","channel":channel,"ts":ts}});
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

fn stage_ready(v: &Value, index: usize) -> bool {
    let held = matches!(index, 1 | 2 | 3 | 8 | 9);
    let loading = matches!(index, 1 | 2 | 8);
    let error = matches!(index, 3 | 4 | 9 | 10);
    let sent = index >= 6;
    v["ledger"]["requests"]
        .as_array()
        .is_some_and(|r| r.len() as u64 == READS[index])
        && v["ledger"]["completed"] == COMPLETED[index]
        && v["ledger"]["cancelled"] == CANCELLED[index]
        && v["ledger"]["held"] == held
        && v["comments"]["loading"] == loading
        && (if error {
            v["comments"]["error"] == NOTICE
        } else {
            v["comments"]["error"].is_null()
        })
        && counts(v, usize::from(sent), usize::from(sent))
}

async fn capture(
    state: Signal<ShellState>,
    model: &Model,
    d: &dioxus::desktop::DesktopContext,
    name: &str,
    ts: &str,
    count: u64,
    proof: &Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    let index = rows.len();
    if STAGES.get(index) != Some(&name) {
        return Err("Reaction refresh stage order".into());
    }
    wait_model(state, model, |v| stage_ready(v, index)).await?;
    stable_drafts(state, model).await?;
    let notice = if matches!(index, 3 | 4 | 9 | 10) {
        format!("&&v.notices.includes({})", json!(NOTICE))
    } else if matches!(index, 1 | 2 | 8) {
        "&&v.notices.some(n=>n.startsWith('Loading comments'))".into()
    } else {
        String::new()
    };
    let expression = format!(
        "(()=>{{const v=window.__listCompose.snapshot(),r=v.rows.find(r=>r.ts==={});v.ok=v.ok&&!!r&&(r.reactions.find(p=>p.name==='wave')?.count||0)==={count}&&v.editor?.editable===true{notice};return v;}})()",
        json!(ts)
    );
    wait_dom(d, &expression).await?;
    // Only transcript scroll is moved for capture. Original editor node and focus remain owned.
    checked(d, &format!("(()=>{{const r=[...document.querySelectorAll('[data-list-comment-ts]')].find(r=>r.dataset.listCommentTs==={});if(!r)return {{ok:false}};(r.querySelector('[data-reaction-name=wave]')||r).scrollIntoView({{block:'nearest'}});return {{ok:true}};}})()", json!(ts))).await?;
    let geometry = wait_dom(d, &expression).await?;
    let before = compose::snapshot(&state.peek(), model);
    if !stage_ready(&before, index)
        || before["noSession"] != true
        || before["noTransport"] != true
        || before["writes"] != 0
        || before["networkCalls"] != 0
    {
        return Err("Reaction refresh fixture changed ownership or stage".into());
    }
    let mut proof = proof.clone();
    proof["journal"] = compose::journal_snapshot(state).await?;
    let expected_blocks = if name == "sent" {
        json!([])
    } else if index >= 7 {
        proof["newerB"].clone()
    } else {
        proof["originalA"].clone()
    };
    if geometry["editor"]["blocks"] != expected_blocks
        || proof["journal"]["blocks"] != expected_blocks
    {
        return Err("Original native and durable draft differed".into());
    }
    let view =
        crate::slack_lists::comments::presentation(&state.peek()).ok_or("Comments missing")?;
    let row = view
        .rows
        .iter()
        .find(|m| m.ts == ts)
        .ok_or("Reaction row missing")?;
    if u64::from(
        row.reactions
            .iter()
            .find(|r| r.name == "wave")
            .map_or(0, |r| r.count),
    ) != count
    {
        return Err("Model reaction count changed".into());
    }
    let probe = json!({"ts":ts,"count":count,"channel":view.channel,
        "editorGeneration":geometry["editor"]["generation"],"hasMore":view.has_more,
        "readRequests":READS[index],"prepared":usize::from(index>=6),"posts":usize::from(index>=6)});
    let capture = harness::capture(d, name).await?;
    if compose::snapshot(&state.peek(), model) != before
        || compose::journal_snapshot(state).await? != proof["journal"]
    {
        return Err(format!("Reaction refresh changed during {name} capture"));
    }
    rows.push(
        json!({"stage":name,"geometry":geometry,"model":before,"proof":proof,
        "reactionProbe":probe,"capture":capture}),
    );
    Ok(())
}

fn clear_read_proof(proof: &mut Value) {
    proof["heldResponse"] = Value::Null;
    proof["serverUpdate"] = Value::Null;
    proof["release"] = Value::Null;
}

async fn hold(
    state: Signal<ShellState>,
    model: &Model,
    d: &dioxus::desktop::DesktopContext,
    index: usize,
    proof: &mut Value,
) -> Result<(), String> {
    fixture::mode(model, fixture::Mode::Held)?;
    click(d, "slack-list-comments-refresh").await?;
    wait_model(state, model, |v| stage_ready(v, index)).await?;
    proof["heldResponse"] = fixture::held_response(&state.peek(), model)?;
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
    let native = wait_dom(d, "(()=>{const v=window.__listCompose.snapshot();v.ok=v.ok&&v.editor?.bold===true;return v;})()").await?;
    let mut proof = json!({"seed":seed,"originalA":native["editor"]["blocks"],"newerB":null,
        "heldResponse":null,"serverUpdate":null,"release":null});
    checked(d, "(()=>{window.__listReactionOriginalEditor=document.getElementById('slack-list-comment-draft');return {ok:!!window.__listReactionOriginalEditor};})()").await?;
    let mut rows = Vec::new();
    let mut events = Vec::new();
    capture(state, &model, d, "draft", FIRST, 2, &proof, &mut rows).await?;
    hold(state, &model, d, 1, &mut proof).await?;
    capture(state, &model, d, "held", FIRST, 2, &proof, &mut rows).await?;
    for (team, generation, channel, ts, user) in [
        ("T1", 7, CHANNEL, FIRST, "U1"),
        ("T_OTHER", 7, CHANNEL, FIRST, "U_RT_REACTOR"),
        ("T1", 6, CHANNEL, FIRST, "U_RT_REACTOR"),
        ("T1", 7, "COTHER", FIRST, "U_RT_REACTOR"),
        ("T1", 7, CHANNEL, "1999999999.000001", "U_RT_REACTOR"),
    ] {
        events.push(inject(
            state,
            "ignored-held",
            team,
            generation,
            channel,
            ts,
            user,
            false,
        )?);
    }
    capture(
        state,
        &model,
        d,
        "ignored-held",
        FIRST,
        2,
        &proof,
        &mut rows,
    )
    .await?;
    events.push(inject(
        state,
        "updated",
        "T1",
        7,
        CHANNEL,
        FIRST,
        "U_RT_REACTOR",
        true,
    )?);
    proof["serverUpdate"] =
        fixture::reaction_readback(&state.peek(), &model, FIRST, "U_RT_REACTOR", "wave")?;
    capture(state, &model, d, "updated", FIRST, 3, &proof, &mut rows).await?;
    proof["release"] = fixture::release(&model)?;
    if proof["release"] != json!({"delivered":false,"cancelledBeforeReply":true}) {
        return Err("Original read was not actually cancelled".into());
    }
    capture(state, &model, d, "released", FIRST, 3, &proof, &mut rows).await?;
    clear_read_proof(&mut proof);
    click(d, "slack-list-comments-refresh").await?;
    capture(state, &model, d, "refreshed", FIRST, 3, &proof, &mut rows).await?;

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
        .ok_or("Accepted timestamp missing")?
        .to_owned();
    capture(state, &model, d, "sent", &sent, 0, &proof, &mut rows).await?;
    write(d).await?;
    type_text(
        d,
        "Newer draft B \u{65e5}\u{1f98a}\nRetain this draft during comment refresh.",
    )
    .await?;
    settled(d).await?;
    let newer = wait_dom(d, "window.__listCompose.snapshot()").await?;
    proof["newerB"] = newer["editor"]["blocks"].clone();
    checked(d, "(()=>{window.__listReactionOriginalEditor=document.getElementById('slack-list-comment-draft');return {ok:!!window.__listReactionOriginalEditor};})()").await?;
    compose::publish(&model)?;
    capture(state, &model, d, "tail-draft", &sent, 0, &proof, &mut rows).await?;
    hold(state, &model, d, 8, &mut proof).await?;
    capture(state, &model, d, "tail-held", &sent, 0, &proof, &mut rows).await?;
    events.push(inject(
        state,
        "tail-updated",
        "T1",
        7,
        CHANNEL,
        &sent,
        "U_RT_REACTOR",
        true,
    )?);
    proof["serverUpdate"] =
        fixture::reaction_readback(&state.peek(), &model, &sent, "U_RT_REACTOR", "wave")?;
    capture(
        state,
        &model,
        d,
        "tail-updated",
        &sent,
        1,
        &proof,
        &mut rows,
    )
    .await?;
    proof["release"] = fixture::release(&model)?;
    if proof["release"] != json!({"delivered":false,"cancelledBeforeReply":true}) {
        return Err("Original accepted-tail read was not actually cancelled".into());
    }
    capture(
        state,
        &model,
        d,
        "tail-released",
        &sent,
        1,
        &proof,
        &mut rows,
    )
    .await?;
    clear_read_proof(&mut proof);
    click(d, "slack-list-comments-refresh").await?;
    capture(
        state,
        &model,
        d,
        "tail-refreshed",
        &sent,
        1,
        &proof,
        &mut rows,
    )
    .await?;
    Ok(
        json!({"ok":true,"case":"reaction-refresh","fixture":readiness["fixture"],
        "readiness":readiness,"stages":rows,"reactionEvents":events,
        "final":compose::snapshot(&state.peek(), &model),
        "initialRecords":fixture::initial_records(&model),"noLiveNetwork":true}),
    )
}
