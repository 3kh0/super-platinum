//! One real native send, immutable held ACK, and parsed socket interleavings.
use super::*;

const FIRST: &str = "1781524801.000001";
const SENT: &str = "1781525001.000001";
const CHANNEL: &str = "CCOMMENT01";
const B: &str =
    "Newer draft B \u{65e5}\u{1f98a}\nRetain this draft after the original acknowledgement.";
const HELD_STAGES: &[&str] = &[
    "draft",
    "post-held",
    "socket",
    "reacted",
    "acknowledged",
    "b-edited",
    "echo",
];
const FIRST_STAGES: &[&str] = &["draft", "acknowledged", "b-edited", "socket", "duplicate"];

fn event_state(v: &Value) -> Value {
    let c = &v["comments"];
    json!({"read":c["read"],"visibleTs":c["visibleTs"],"liveTs":c["liveTs"],
        "acceptedTs":c["acceptedTs"],"liveReactions":c["liveReactions"],"deleted":c["deleted"],
        "reactions":c["reactions"],"reveal":c["reveal"]})
}

fn inject(
    mut state: Signal<ShellState>,
    model: &Model,
    stage: &str,
    frame: Value,
    expected: bool,
) -> Result<Value, String> {
    let event = super_platinum_core::slack::realtime::parse_event(&frame.to_string())
        .ok_or("ACK/socket frame failed parsing")?;
    let mut shell = state.write();
    if !compose::enabled(&shell) || shell.core.session.is_some() || shell.core.transport.is_some() {
        return Err("Closed ACK/socket fixture required".into());
    }
    let before = compose::snapshot(&shell, model);
    if crate::slack_lists::events::prepare(&mut shell, "T1", 7, &event) {
        return Err("ACK/socket event unexpectedly requested List refresh".into());
    }
    let after = compose::snapshot(&shell, model);
    let changed = before != after;
    if changed != expected || before["draftStore"] != after["draftStore"] {
        return Err(format!(
            "ACK/socket event {stage}: before={before} after={after}"
        ));
    }
    Ok(
        json!({"stage":stage,"team":"T1","generation":7,"frame":frame,"parsed":true,
        "changed":changed,"before":event_state(&before),"after":event_state(&after)}),
    )
}

fn held(case: &str, index: usize) -> bool {
    case != "ack-first" && (1..=3).contains(&index)
}
fn acknowledged(case: &str, index: usize) -> bool {
    index >= if case == "ack-first" { 1 } else { 4 }
}
fn newer(case: &str, index: usize) -> bool {
    index >= if case == "ack-first" { 2 } else { 5 }
}
fn visible(case: &str, index: usize) -> Value {
    if index == 0 || (case != "ack-first" && index == 1) {
        json!([FIRST])
    } else {
        json!([FIRST, SENT])
    }
}
fn reactions(case: &str, index: usize) -> Value {
    let wave = match case {
        "ack-socket-remove" => index == 2,
        "ack-socket-add" => index >= 3,
        _ => false,
    };
    if wave {
        json!([{"name":"wave","count":1}])
    } else {
        json!([])
    }
}
fn ready(v: &Value, case: &str, index: usize) -> bool {
    let sent = index > 0;
    let ack = acknowledged(case, index);
    let live = case != "ack-first" && (2..=3).contains(&index);
    let reads = if sent { 8 } else { 4 };
    let drafts = v["draftStore"]["drafts"].as_array();
    counts(v, usize::from(sent), usize::from(sent))
        && v["ledger"]["requests"]
            .as_array()
            .is_some_and(|r| r.len() == reads)
        && v["ledger"]["completed"] == reads
        && v["ledger"]["cancelled"] == 0
        && v["comments"]["loading"] == false
        && v["comments"]["error"].is_null()
        && v["composeLedger"]["heldPost"] == held(case, index)
        && v["comments"]["visibleTs"] == visible(case, index)
        && v["comments"]["liveTs"] == if live { json!([SENT]) } else { json!([]) }
        && v["comments"]["acceptedTs"] == if ack { json!([SENT]) } else { json!([]) }
        && v["comments"]["liveReactions"]
            == if live && index == 3 {
                json!([SENT])
            } else {
                json!([])
            }
        && v["comments"]["deleted"] == json!([])
        && drafts.is_some_and(|r| {
            r.len() == 1
                && r[0]["busy"] == held(case, index)
                && r[0]["saving"] == false
                && r[0]["loading"] == false
                && r[0]["terminalPending"] == false
        })
}

async fn capture(
    state: Signal<ShellState>,
    model: &Model,
    d: &dioxus::desktop::DesktopContext,
    case: &str,
    name: &str,
    proof: &Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    let index = rows.len();
    let stages = if case == "ack-first" {
        FIRST_STAGES
    } else {
        HELD_STAGES
    };
    if stages.get(index) != Some(&name) {
        return Err("ACK/socket stage order".into());
    }
    wait_model(state, model, |v| ready(v, case, index)).await?;
    if !held(case, index) {
        stable_drafts(state, model).await?;
    }
    let expected_blocks = if newer(case, index) {
        proof["newerB"].clone()
    } else if acknowledged(case, index) {
        json!([])
    } else {
        proof["originalA"].clone()
    };
    let same = if acknowledged(case, index) && !newer(case, index) {
        "false"
    } else {
        "true"
    };
    let editable = !held(case, index);
    let disabled = held(case, index) || (acknowledged(case, index) && !newer(case, index));
    let expected_rows = visible(case, index);
    let expected_reactions = reactions(case, index);
    // Native JSON insertion order differs from serde_json object key order.
    // Await reaction rendering using ordered primitive pairs; compare the full
    // objects and opaque editor blocks structurally below, before capture.
    let reaction_pairs = expected_reactions
        .as_array()
        .unwrap()
        .iter()
        .map(|r| json!([r["name"], r["count"]]))
        .collect::<Vec<_>>();
    let reaction_pairs = json!(reaction_pairs);
    let notice = if acknowledged(case, index) && !newer(case, index) {
        "&&v.notices.includes('Comment sent.')"
    } else {
        ""
    };
    let expression = format!(
        "(()=>{{const v=window.__listCompose.snapshot();const row=v.rows.find(r=>r.ts==={});v.ok=v.ok&&JSON.stringify(v.rows.map(r=>r.ts))===JSON.stringify({expected_rows})&&v.editor?.sameNode==={same}&&v.editor?.editable==={editable}&&document.getElementById('slack-list-comment-send')?.disabled==={disabled}&&(!row||JSON.stringify(row.reactions.map(r=>[r.name,r.count]))===JSON.stringify({reaction_pairs})){notice};return v;}})()",
        json!(SENT)
    );
    wait_dom(d, &expression).await?;
    let ts = if index == 0 || (case != "ack-first" && index == 1) {
        FIRST
    } else {
        SENT
    };
    checked(d, &format!("(()=>{{const r=[...document.querySelectorAll('[data-list-comment-ts]')].find(r=>r.dataset.listCommentTs==={});if(!r)return {{ok:false}};r.scrollIntoView({{block:'nearest'}});return {{ok:true}};}})()",json!(ts))).await?;
    let geometry = wait_dom(d, &expression).await?;
    let before = compose::snapshot(&state.peek(), model);
    let mut proof = proof.clone();
    proof["journal"] = compose::journal_snapshot(state).await?;
    if !ready(&before, case, index)
        || before["noSession"] != true
        || before["noTransport"] != true
        || before["networkCalls"] != 0
        || before["writes"] != 0
        || geometry["editor"]["blocks"] != expected_blocks
        || geometry["rows"].as_array().is_none_or(|rows| {
            rows.iter()
                .filter(|r| r["ts"] == SENT)
                .any(|r| r["reactions"] != expected_reactions)
        })
        || proof["journal"]["blocks"] != expected_blocks
        || (index > 0 && before["composeLedger"]["messages"] != json!([proof["originalMessage"]]))
    {
        return Err(format!(
            "ACK/socket stage {name} changed draft, raw ACK or ownership"
        ));
    }
    let view =
        crate::slack_lists::comments::presentation(&state.peek()).ok_or("Comments missing")?;
    let probe = json!({"ts":ts,"visibleTs":before["comments"]["visibleTs"],"liveTs":before["comments"]["liveTs"],
        "acceptedTs":before["comments"]["acceptedTs"],"liveReactions":before["comments"]["liveReactions"],"channel":view.channel,
        "editorGeneration":geometry["editor"]["generation"],"hasMore":view.has_more,"readRequests":if index==0 {4}else{8},"prepared":if index==0 {0}else{1},"posts":if index==0 {0}else{1}});
    let capture = harness::capture(d, name).await?;
    if compose::snapshot(&state.peek(), model) != before
        || compose::journal_snapshot(state).await? != proof["journal"]
    {
        return Err(format!("ACK/socket changed during {name} capture"));
    }
    rows.push(json!({"stage":name,"geometry":geometry,"model":before,"proof":proof,"ackProbe":probe,"capture":capture}));
    Ok(())
}

pub(super) async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    case: &str,
) -> Result<Value, String> {
    let readiness = harness::wait_runner(state).await?;
    let model = fixture::begin(&mut state.write())?;
    state
        .write()
        .core
        .workspaces
        .get_mut("T1")
        .ok_or("Workspace missing")?
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
    let native = wait_dom(d,"(()=>{const v=window.__listCompose.snapshot();v.ok=v.ok&&v.editor?.bold===true;return v;})()").await?;
    let mut proof = json!({"seed":seed,"originalA":native["editor"]["blocks"],"newerB":null,"originalMessage":null,"release":null});
    witness(d).await?;
    let mut rows = Vec::new();
    let mut events = Vec::new();
    capture(state, &model, d, case, "draft", &proof, &mut rows).await?;
    compose::mode(
        &model,
        if case == "ack-first" {
            Mode::Success
        } else {
            Mode::HeldSuccess
        },
    )?;
    click(d, "slack-list-comment-send").await?;
    let sent = wait_model(state, &model, |v| {
        counts(v, 1, 1)
            && v["composeLedger"]["messages"]
                .as_array()
                .is_some_and(|r| r.len() == 1)
    })
    .await?;
    proof["originalMessage"] = sent["composeLedger"]["messages"][0].clone();
    if proof["originalMessage"]["ts"] != SENT {
        return Err("Unexpected first post timestamp".into());
    }
    if case != "ack-first" {
        capture(state, &model, d, case, "post-held", &proof, &mut rows).await?;
        events.push(inject(
            state,
            &model,
            "socket",
            proof["originalMessage"].clone(),
            true,
        )?);
        capture(state, &model, d, case, "socket", &proof, &mut rows).await?;
        let kind = if case == "ack-socket-remove" {
            "reaction_removed"
        } else {
            "reaction_added"
        };
        events.push(inject(state,&model,"reacted",json!({"type":kind,"user":"U_RT_REACTOR","reaction":"wave","item":{"type":"message","channel":CHANNEL,"ts":SENT}}),true)?);
        capture(state, &model, d, case, "reacted", &proof, &mut rows).await?;
        proof["release"] = compose::release_post(&model)?;
        if proof["release"] != json!({"delivered":true,"cancelledBeforeReply":false}) {
            return Err("Held post was cancelled".into());
        }
    }
    capture(state, &model, d, case, "acknowledged", &proof, &mut rows).await?;
    type_text(d, B).await?;
    settled(d).await?;
    let native = wait_dom(d,"(()=>{const v=window.__listCompose.snapshot();v.ok=v.ok&&v.editor?.editable===true;return v;})()").await?;
    proof["newerB"] = native["editor"]["blocks"].clone();
    witness(d).await?;
    capture(state, &model, d, case, "b-edited", &proof, &mut rows).await?;
    if case == "ack-first" {
        events.push(inject(
            state,
            &model,
            "socket",
            proof["originalMessage"].clone(),
            false,
        )?);
        capture(state, &model, d, case, "socket", &proof, &mut rows).await?;
        let mut conflicting = proof["originalMessage"].clone();
        conflicting["text"] = json!("Conflicting duplicate must not replace the accepted comment");
        conflicting["blocks"] = json!([]);
        events.push(inject(state, &model, "duplicate", conflicting, false)?);
        capture(state, &model, d, case, "duplicate", &proof, &mut rows).await?;
    } else {
        events.push(inject(
            state,
            &model,
            "echo",
            proof["originalMessage"].clone(),
            false,
        )?);
        capture(state, &model, d, case, "echo", &proof, &mut rows).await?;
    }
    Ok(
        json!({"ok":true,"case":case,"fixture":readiness["fixture"],"readiness":readiness,"stages":rows,"ackEvents":events,
        "final":compose::snapshot(&state.peek(),&model),"initialRecords":fixture::initial_records(&model),"noLiveNetwork":true}),
    )
}

async fn witness(d: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    checked(d,"(()=>{window.__listReactionOriginalEditor=document.getElementById('slack-list-comment-draft');return {ok:!!window.__listReactionOriginalEditor};})()").await?;
    Ok(())
}
