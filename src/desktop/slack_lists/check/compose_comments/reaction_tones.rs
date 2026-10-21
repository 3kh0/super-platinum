//! Tone display preserves exact raw reaction keys without enabling any action.
use super::*;
use crate::slack_lists::comments::{fixture::reaction_tones as tones, reaction_actions};
const STAGES: &[&str] = &["draft", "socket", "refreshed", "offline"];
const OFFLINE: &str = "You are offline. Previously loaded comments are retained.";
fn glyphs() -> Value {
    json!([
        "\u{1f44d}\u{1f3fc}",
        "\u{1f44d}\u{1f3fc}",
        "\u{1faf1}\u{1f3fb}\u{200d}\u{1faf2}\u{1f3ff}",
        "\u{1f44b}",
        null,
        ":not_real::skin-tone-3:"
    ])
}
fn snapshot(state: Signal<ShellState>, model: &Model) -> Result<Value, String> {
    let shell = state.peek();
    let mut value = compose::snapshot(&shell, model);
    let view = crate::slack_lists::comments::presentation(&shell).ok_or("Tone comments missing")?;
    let row = view
        .rows
        .iter()
        .find(|r| r.ts == tones::FIRST)
        .ok_or("Tone row missing")?;
    value["toneProbe"] = json!(row.reactions.iter().map(|r|json!({"name":r.name,"glyph":r.glyph,"media":r.media.is_some(),"count":r.count,"own":r.own,"available":reaction_actions::available(&shell,view.generation,tones::FIRST,&r.name)})).collect::<Vec<_>>());
    value["reactionActions"] = reaction_actions::snapshot(&shell);
    value["toneMedia"] = tones::media(&shell)?;
    Ok(value)
}
async fn capture(
    state: Signal<ShellState>,
    model: &Model,
    d: &dioxus::desktop::DesktopContext,
    index: usize,
    proof: &Value,
    rows: &mut Vec<Value>,
) -> Result<(), String> {
    let reads = [4, 4, 8, 12][index];
    wait_model(state, model, |v| {
        v["ledger"]["requests"]
            .as_array()
            .is_some_and(|r| r.len() == reads)
            && v["ledger"]["completed"] == reads
            && v["comments"]["loading"] == false
            && v["comments"]["error"]
                == if index == 3 {
                    json!(OFFLINE)
                } else {
                    Value::Null
                }
    })
    .await?;
    stable_drafts(state, model).await?;
    checked(d,"(()=>{const r=document.querySelector('[data-list-comment-ts=\"1781524801.000001\"] .message-actions');if(!r)return {ok:false};r.scrollIntoView({block:'nearest'});return {ok:true};})()").await?;
    let expression = format!(
        "({})(({}),{},{},{},{})",
        include_str!("reaction_tones.js"),
        include_str!("reaction_actions/text.js"),
        json!(tones::NAMES),
        glyphs(),
        index > 0,
        if index == 3 {
            json!(OFFLINE)
        } else {
            Value::Null
        }
    );
    let geometry = wait_dom(d, &expression).await?;
    let before = snapshot(state, model)?;
    let mut proof = proof.clone();
    proof["journal"] = compose::journal_snapshot(state).await?;
    if before["noSession"] != true
        || before["noTransport"] != true
        || before["networkCalls"] != 0
        || before["writes"] != 0
        || !counts(&before, 0, 0)
        || before["reactionActions"]["attempts"] != json!([])
        || before["reactionLedger"]["prepared"] != json!([])
        || before["reactionLedger"]["issued"] != json!([])
        || geometry["editor"]["blocks"] != proof["originalA"]
        || proof["journal"]["blocks"] != proof["originalA"]
        || before["toneMedia"]["ready"] != true
    {
        return Err("Tone display lost closed source/draft custody".into());
    }
    let probe = before["toneProbe"].as_array().ok_or("Tone model missing")?;
    if probe.len() != tones::NAMES.len()
        || probe.iter().enumerate().any(|(i, p)| {
            p["name"] != tones::NAMES[i]
                || p["glyph"] != glyphs()[i]
                || p["media"] != (i == 4)
                || p["available"] != false
                || p["own"] != false
                || p["count"] != if index > 0 && i == 1 { 2 } else { 1 }
        })
    {
        return Err("Tone model changed raw name/glyph/policy".into());
    }
    if index < 2 && before["comments"]["page"] != proof["initialResponse"]
        || index >= 2 && before["comments"]["page"] != proof["serverResponse"]
    {
        return Err("Tone original read response changed".into());
    }
    let capture = harness::capture(d, STAGES[index]).await?;
    if snapshot(state, model)? != before
        || compose::journal_snapshot(state).await? != proof["journal"]
    {
        return Err("Tone state changed during capture".into());
    }
    rows.push(json!({"stage":STAGES[index],"model":before,"geometry":geometry,"proof":proof,"capture":capture}));
    Ok(())
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let (readiness, model, mut proof) = edits::setup(state, d).await?;
    proof["initialResponse"] = compose::snapshot(&state.peek(), &model)["comments"]["page"].clone();
    proof["toneEvent"] = Value::Null;
    let mut rows = vec![];
    capture(state, &model, d, 0, &proof, &mut rows).await?;
    let frame = json!({"type":"reaction_added","user":"U_RT_REACTOR","reaction":"+1::skin-tone-3","item":{"type":"message","channel":"CCOMMENT01","ts":tones::FIRST}});
    let event = super_platinum_core::slack::realtime::parse_event(&frame.to_string())
        .ok_or("Tone frame not parsed")?;
    let before = snapshot(state, &model)?;
    if crate::slack_lists::events::prepare(&mut state.write(), "T1", 7, &event) {
        return Err("Tone socket unexpectedly requested read".into());
    }
    let after = snapshot(state, &model)?;
    if before["comments"]["page"] != after["comments"]["page"]
        || before["draftStore"] != after["draftStore"]
        || before["ledger"] != after["ledger"]
    {
        return Err("Tone socket changed original read/draft".into());
    }
    proof["toneEvent"] = json!({"team":"T1","generation":7,"frame":frame,"parsed":true,"before":before["toneProbe"],"after":after["toneProbe"]});
    capture(state, &model, d, 1, &proof, &mut rows).await?;
    proof["serverResponse"] = tones::server(&state.peek(), &model)?;
    click(d, "slack-list-comments-refresh").await?;
    capture(state, &model, d, 2, &proof, &mut rows).await?;
    fixture::mode(&model, fixture::Mode::Offline)?;
    click(d, "slack-list-comments-refresh").await?;
    capture(state, &model, d, 3, &proof, &mut rows).await?;
    Ok(
        json!({"ok":true,"case":"reaction-tones","fixture":readiness["fixture"],"readiness":readiness,"stages":rows,"final":snapshot(state,&model)?,"initialRecords":fixture::initial_records(&model),"noLiveNetwork":true}),
    )
}
