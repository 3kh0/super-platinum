//! Edited socket body/reactions outrank the immutable original POST ACK.
use super::edits::{CHANNEL, Expected, FIRST, SENT, blocks, capture, changed, inject};
use super::*;
const STAGES: &[&str] = &[
    "draft",
    "post-held",
    "socket",
    "edited",
    "reacted",
    "acknowledged",
    "b-edited",
    "echo",
    "tail-edited",
    "readback",
];
const B: &str =
    "Newer draft B \u{65e5}\u{1f98a}\nRetain this draft after the original acknowledgement.";
fn expected(index: usize, proof: &Value) -> Expected<'static> {
    let reads = if index == 0 {
        4
    } else if index == 9 {
        12
    } else {
        8
    };
    let (ts, text, bold, italic, wave) = match index {
        0 | 1 => (FIRST, "First page comment.", true, false, 2),
        2 => (SENT, "Final review \u{4e2d}\u{1f642}", true, false, 1),
        3 => (SENT, "Socket edited reply.", true, false, 1),
        4..=7 => (SENT, "Socket edited reply.", true, false, 0),
        8 => (SENT, "Accepted tail edited.", false, true, 0),
        _ => (SENT, "Canonical sent reply.", false, true, 0),
    };
    Expected {
        reads,
        completed: reads,
        cancelled: 0,
        held: false,
        loading: false,
        posts: usize::from(index > 0),
        busy: (1..=4).contains(&index),
        same: index != 5,
        ts,
        text,
        bold,
        italic,
        wave,
        notice: None,
        blocks: if index >= 6 {
            proof["newerB"].clone()
        } else if index == 5 {
            json!([])
        } else {
            proof["originalA"].clone()
        },
    }
}
fn source_ready(v: &Value, index: usize) -> bool {
    let live = (2..=4).contains(&index);
    let accepted = (5..=8).contains(&index);
    v["composeLedger"]["heldPost"] == (1..=4).contains(&index)
        && v["comments"]["visibleTs"]
            == if index < 2 {
                json!([FIRST])
            } else {
                json!([FIRST, SENT])
            }
        && v["comments"]["liveTs"] == if live { json!([SENT]) } else { json!([]) }
        && v["comments"]["acceptedTs"] == if accepted { json!([SENT]) } else { json!([]) }
        && v["comments"]["editCustody"]
            == if matches!(index, 3 | 4) {
                json!([SENT])
            } else {
                json!([])
            }
        && v["comments"]["liveReactions"] == if index == 4 { json!([SENT]) } else { json!([]) }
}
async fn stage(
    state: Signal<ShellState>,
    model: &Model,
    d: &dioxus::desktop::DesktopContext,
    index: usize,
    proof: &Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    wait_model(state, model, |v| source_ready(v, index)).await?;
    capture(
        state,
        model,
        d,
        STAGES[index],
        proof,
        rows,
        expected(index, proof),
    )
    .await
}
pub(super) async fn run(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let (readiness, model, mut proof) = edits::setup(state, d).await?;
    let mut rows = Vec::new();
    let mut events = Vec::new();
    stage(state, &model, d, 0, &proof, &mut rows).await?;
    compose::mode(&model, Mode::HeldSuccess)?;
    click(d, "slack-list-comment-send").await?;
    let sent = wait_model(state, &model, |v| {
        counts(v, 1, 1)
            && v["composeLedger"]["messages"]
                .as_array()
                .is_some_and(|m| m.len() == 1)
    })
    .await?;
    proof["originalMessage"] = sent["composeLedger"]["messages"][0].clone();
    if proof["originalMessage"]["ts"] != SENT
        || proof["originalMessage"]["reactions"]
            != json!([{"name":"wave","count":1,"users":["U_RT_REACTOR"]}])
    {
        return Err("Original ACK was not initialized before hold".into());
    }
    stage(state, &model, d, 1, &proof, &mut rows).await?;
    events.push(inject(
        state,
        &model,
        "socket",
        "T1",
        7,
        proof["originalMessage"].clone(),
        true,
    )?);
    stage(state, &model, d, 2, &proof, &mut rows).await?;
    let mut edited = changed(SENT, "Socket edited reply.");
    edited["message"]["user"] = proof["originalMessage"]["user"].clone();
    edited["message"]["blocks"] = blocks("Socket edited reply.", true);
    edited["message"]["edited"] = json!({"ts":"1781525200.000001"});
    events.push(inject(state, &model, "edited", "T1", 7, edited, true)?);
    stage(state, &model, d, 3, &proof, &mut rows).await?;
    events.push(inject(state,&model,"reacted","T1",7,json!({"type":"reaction_removed","user":"U_RT_REACTOR","reaction":"wave","item":{"type":"message","channel":CHANNEL,"ts":SENT}}),true)?);
    stage(state, &model, d, 4, &proof, &mut rows).await?;
    proof["release"] = compose::release_post(&model)?;
    if proof["release"] != json!({"delivered":true,"cancelledBeforeReply":false}) {
        return Err("Original held ACK did not complete".into());
    }
    stage(state, &model, d, 5, &proof, &mut rows).await?;
    type_text(d, B).await?;
    settled(d).await?;
    let native=wait_dom(d,"(()=>{const v=window.__listCompose.snapshot();v.ok=v.ok&&v.editor?.editable===true;return v;})()").await?;
    proof["newerB"] = native["editor"]["blocks"].clone();
    edits::witness(d).await?;
    stage(state, &model, d, 6, &proof, &mut rows).await?;
    events.push(inject(
        state,
        &model,
        "echo",
        "T1",
        7,
        proof["originalMessage"].clone(),
        false,
    )?);
    stage(state, &model, d, 7, &proof, &mut rows).await?;
    let mut tail = changed(SENT, "Accepted tail edited.");
    tail["message"]["user"] = proof["originalMessage"]["user"].clone();
    tail["message"]["blocks"] = blocks("Accepted tail edited.", false);
    events.push(inject(state, &model, "tail-edited", "T1", 7, tail, true)?);
    stage(state, &model, d, 8, &proof, &mut rows).await?;
    let raw = json!({"type":"message","channel":CHANNEL,"ts":SENT,"thread_ts":fixture::ROOT,"user":proof["originalMessage"]["user"],"client_msg_id":proof["originalMessage"]["client_msg_id"],"text":"Canonical sent reply.","blocks":blocks("Canonical sent reply.",false),"reactions":[]});
    proof["serverResponse"] = fixture::publish_edit_readback(&state.peek(), &model, vec![raw])?;
    click(d, "slack-list-comments-refresh").await?;
    stage(state, &model, d, 9, &proof, &mut rows).await?;
    Ok(edits::result(
        "edit-ack", readiness, rows, events, state, &model,
    ))
}
