//! Actual parsed arrivals and canonical readback share one mounted rich draft.
use super::*;

const FIRST: &str = "1781524801.000001";
const CHANNEL: &str = "CCOMMENT01";
const NOTICE: &str =
    "Newer comment updates arrived. Loaded comments are retained. Refresh to try again.";
const LIVE1: &str = "1781524804.000001";
const LIVE2: &str = "1781524805.000001";
const STAGES: &[&str] = &[
    "draft",
    "arrived",
    "duplicate",
    "foreign",
    "held",
    "arrived-held",
    "released",
    "stale-refreshed",
    "reacted",
    "deleted",
    "readback",
    "final-duplicate",
];
const READS: &[u64] = &[4, 4, 4, 4, 8, 8, 8, 12, 12, 12, 16, 16];
const COMPLETED: &[u64] = &[4, 4, 4, 4, 7, 8, 8, 12, 12, 12, 16, 16];
const CANCELLED: &[u64] = &[0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1];
fn event_state(v: &Value) -> Value {
    let c = &v["comments"];
    json!({"read":c["read"],"visibleTs":c["visibleTs"],"liveTs":c["liveTs"],"deleted":c["deleted"],"reactions":c["reactions"]})
}

fn inject(
    mut state: Signal<ShellState>,
    stage: &str,
    team: &str,
    generation: u64,
    frame: Value,
    expected: bool,
) -> Result<Value, String> {
    let event = super_platinum_core::slack::realtime::parse_event(&frame.to_string())
        .ok_or("Incoming fixture frame failed parsing")?;
    let mut shell = state.write();
    if !compose::enabled(&shell) || shell.core.session.is_some() || shell.core.transport.is_some() {
        return Err("Disconnected incoming fixture required".into());
    }
    let model = shell
        .slack_lists
        .comments
        .fixture
        .clone()
        .ok_or("Fixture missing")?;
    let before = compose::snapshot(&shell, &model);
    if crate::slack_lists::events::prepare(&mut shell, team, generation, &event) {
        return Err("Incoming fixture unexpectedly requested List refresh".into());
    }
    let after = compose::snapshot(&shell, &model);
    let changed = before != after;
    if changed != expected {
        return Err(format!(
            "Unexpected incoming change at {stage}: before={} after={}",
            event_state(&before),
            event_state(&after)
        ));
    }
    Ok(
        json!({"stage":stage,"team":team,"generation":generation,"frame":frame,"parsed":true,"changed":changed,"before":event_state(&before),"after":event_state(&after)}),
    )
}
fn visible(index: usize) -> Vec<&'static str> {
    if index == 0 {
        vec![FIRST]
    } else if (5..=8).contains(&index) {
        vec![FIRST, LIVE1, LIVE2]
    } else {
        vec![FIRST, LIVE1]
    }
}
fn live(index: usize) -> Vec<&'static str> {
    if index == 0 || index >= 10 {
        vec![]
    } else if (5..=8).contains(&index) {
        vec![LIVE1, LIVE2]
    } else {
        vec![LIVE1]
    }
}

fn stage_ready(v: &Value, index: usize) -> bool {
    v["ledger"]["requests"]
        .as_array()
        .is_some_and(|r| r.len() as u64 == READS[index])
        && v["ledger"]["completed"] == COMPLETED[index]
        && v["ledger"]["cancelled"] == CANCELLED[index]
        && v["ledger"]["held"] == matches!(index, 4 | 5)
        && v["comments"]["loading"] == (index == 4)
        && (if matches!(index, 5 | 6) {
            v["comments"]["error"] == NOTICE
        } else {
            v["comments"]["error"].is_null()
        })
        && counts(v, 0, 0)
        && v["comments"]["visibleTs"] == json!(visible(index))
        && v["comments"]["liveTs"] == json!(live(index))
        && v["comments"]["deleted"]
            == if index >= 9 {
                json!([LIVE2])
            } else {
                json!([])
            }
}

async fn capture(
    state: Signal<ShellState>,
    model: &Model,
    d: &dioxus::desktop::DesktopContext,
    name: &str,
    proof: &Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    let index = rows.len();
    if STAGES.get(index) != Some(&name) {
        return Err("Incoming stage order".into());
    }
    wait_model(state, model, |v| stage_ready(v, index)).await?;
    stable_drafts(state, model).await?;
    let notice = if matches!(index, 5 | 6) {
        format!("&&v.notices.includes({})", json!(NOTICE))
    } else if index == 4 {
        "&&v.notices.some(n=>n.startsWith('Loading comments'))".into()
    } else {
        String::new()
    };
    let ts = if index == 0 {
        FIRST
    } else if (5..=8).contains(&index) {
        LIVE2
    } else {
        LIVE1
    };
    let expected = json!(visible(index));
    let expression = format!(
        "(()=>{{const v=window.__listCompose.snapshot();v.ok=v.ok&&JSON.stringify(v.rows.map(r=>r.ts))===JSON.stringify({expected})&&v.editor?.sameNode===true&&v.editor?.editable===true{notice};return v;}})()"
    );
    wait_dom(d, &expression).await?;
    // Capture explicitly moves transcript scroll, without changing the editor.
    checked(d,&format!("(()=>{{const r=[...document.querySelectorAll('[data-list-comment-ts]')].find(r=>r.dataset.listCommentTs==={});if(!r)return {{ok:false}};r.scrollIntoView({{block:'nearest'}});return {{ok:true}};}})()",json!(ts))).await?;
    let geometry = wait_dom(d, &expression).await?;
    let before = compose::snapshot(&state.peek(), model);
    if !stage_ready(&before, index)
        || before["noSession"] != true
        || before["noTransport"] != true
        || before["writes"] != 0
        || before["networkCalls"] != 0
    {
        return Err("Incoming fixture changed ownership or stage".into());
    }
    let mut proof = proof.clone();
    proof["journal"] = compose::journal_snapshot(state).await?;
    if geometry["editor"]["blocks"] != proof["originalA"]
        || proof["journal"]["blocks"] != proof["originalA"]
    {
        return Err("Incoming event changed native or durable draft".into());
    }
    let view =
        crate::slack_lists::comments::presentation(&state.peek()).ok_or("Comments missing")?;
    proof["serverResponse"] = fixture::incoming_server_response(&state.peek(), model)?;
    let probe = json!({"ts":ts,"visibleTs":before["comments"]["visibleTs"],"liveTs":before["comments"]["liveTs"],"channel":view.channel,
        "editorGeneration":geometry["editor"]["generation"],"hasMore":view.has_more,"readRequests":READS[index],"prepared":0,"posts":0});
    let capture = harness::capture(d, name).await?;
    if compose::snapshot(&state.peek(), model) != before
        || compose::journal_snapshot(state).await? != proof["journal"]
    {
        return Err(format!("Incoming changed during {name} capture"));
    }
    rows.push(
        json!({"stage":name,"geometry":geometry,"model":before,"proof":proof,
        "incomingProbe":probe,"capture":capture}),
    );
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
    let mut proof = json!({"seed":seed,"originalA":native["editor"]["blocks"],
        "heldResponse":null,"release":null});
    checked(d, "(()=>{window.__listReactionOriginalEditor=document.getElementById('slack-list-comment-draft');return {ok:!!window.__listReactionOriginalEditor};})()").await?;
    let mut rows = Vec::new();
    let mut events = Vec::new();
    capture(state, &model, d, "draft", &proof, &mut rows).await?;
    events.push(inject(
        state,
        "arrived",
        "T1",
        7,
        fixture::incoming_message(1),
        true,
    )?);
    capture(state, &model, d, "arrived", &proof, &mut rows).await?;
    events.push(inject(
        state,
        "duplicate",
        "T1",
        7,
        fixture::incoming_message(1),
        false,
    )?);
    let mut conflicting = fixture::incoming_message(1);
    conflicting["text"] = json!("Conflicting duplicate must be ignored");
    conflicting["blocks"] = json!([]);
    events.push(inject(state, "duplicate", "T1", 7, conflicting, false)?);
    capture(state, &model, d, "duplicate", &proof, &mut rows).await?;
    for index in 0..6 {
        let mut frame = fixture::incoming_message(1);
        frame["ts"] = json!("1781524806.000001");
        let (team, generation) = match index {
            0 => ("T_OTHER", 7),
            1 => ("T1", 6),
            _ => ("T1", 7),
        };
        match index {
            2 => frame["channel"] = json!("COTHER"),
            3 => frame["thread_ts"] = json!("1781524999.000001"),
            4 => frame["ts"] = json!("not-a-timestamp"),
            5 => frame["_hidden_stub"] = json!(true),
            _ => (),
        }
        events.push(inject(state, "foreign", team, generation, frame, false)?);
    }
    capture(state, &model, d, "foreign", &proof, &mut rows).await?;
    fixture::mode(&model, fixture::Mode::Held)?;
    click(d, "slack-list-comments-refresh").await?;
    wait_model(state, &model, |v| stage_ready(v, 4)).await?;
    proof["heldResponse"] = fixture::held_response(&state.peek(), &model)?;
    capture(state, &model, d, "held", &proof, &mut rows).await?;
    events.push(inject(
        state,
        "arrived-held",
        "T1",
        7,
        fixture::incoming_message(2),
        true,
    )?);
    capture(state, &model, d, "arrived-held", &proof, &mut rows).await?;
    proof["release"] = fixture::release(&model)?;
    if proof["release"] != json!({"delivered":false,"cancelledBeforeReply":true}) {
        return Err("Incoming delta did not cancel original HTTP read".into());
    }
    capture(state, &model, d, "released", &proof, &mut rows).await?;
    proof["heldResponse"] = Value::Null;
    proof["release"] = Value::Null;
    click(d, "slack-list-comments-refresh").await?;
    capture(state, &model, d, "stale-refreshed", &proof, &mut rows).await?;
    events.push(inject(state,"reacted","T1",7,json!({"type":"reaction_added","user":"U_RT_REACTOR","reaction":"wave","item":{"type":"message","channel":CHANNEL,"ts":LIVE2}}),true)?);
    capture(state, &model, d, "reacted", &proof, &mut rows).await?;
    events.push(inject(state,"deleted","T1",7,json!({"type":"message","subtype":"message_deleted","channel":CHANNEL,"deleted_ts":LIVE2,"event_ts":"1781525900.000001"}),true)?);
    capture(state, &model, d, "deleted", &proof, &mut rows).await?;
    fixture::publish_incoming_readback(&state.peek(), &model)?;
    click(d, "slack-list-comments-refresh").await?;
    capture(state, &model, d, "readback", &proof, &mut rows).await?;
    events.push(inject(
        state,
        "final-duplicate",
        "T1",
        7,
        fixture::incoming_message(1),
        false,
    )?);
    capture(state, &model, d, "final-duplicate", &proof, &mut rows).await?;
    Ok(
        json!({"ok":true,"case":"incoming","fixture":readiness["fixture"],"readiness":readiness,"stages":rows,"incomingEvents":events,
        "final":compose::snapshot(&state.peek(),&model),"initialRecords":fixture::initial_records(&model),"noLiveNetwork":true}),
    )
}
