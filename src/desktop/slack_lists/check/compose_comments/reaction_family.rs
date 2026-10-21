//! Original raw-base families through native clicks and per-child closed replies.
use super::*;
use crate::slack_lists::comments::{
    fixture::{reaction_actions as dispatch, reaction_family as family},
    reaction_actions as actions,
};
mod failures;
#[path = "reaction_actions/stability.rs"]
mod stability;
mod success;
fn snapshot(state: Signal<ShellState>, model: &Model) -> Result<Value, String> {
    let shell = state.peek();
    let mut v = compose::snapshot(&shell, model);
    let view = crate::slack_lists::comments::presentation(&shell).ok_or("Family view missing")?;
    let row = view
        .rows
        .iter()
        .find(|r| r.ts == family::FIRST)
        .ok_or("Family row missing")?;
    v["reactionActions"] = actions::snapshot(&shell);
    v["familyProbe"]=json!(row.reactions.iter().map(|r|json!({"name":r.name,"count":r.count,"own":r.own,"glyph":r.glyph,"available":actions::available(&shell,view.generation,family::FIRST,&r.name)})).collect::<Vec<_>>());
    v["actionProbe"] = v["familyProbe"]
        .as_array()
        .and_then(|rows| rows.iter().find(|r| r["name"] == family::SELECTED))
        .cloned()
        .ok_or("Selected family pill missing")?;
    v["familyStatus"] = json!(actions::status(&shell, view.generation));
    Ok(v)
}
struct Expected {
    reads: usize,
    prepared: usize,
    completed: usize,
    phases: Vec<&'static str>,
    removed: Vec<&'static str>,
    held: bool,
    read_held: bool,
    loading: bool,
    cancelled: u64,
    disabled: bool,
    full_reads: usize,
}
impl Expected {
    fn ready(&self, v: &Value) -> bool {
        let len = |x: &Value, n| x.as_array().is_some_and(|a| a.len() == n);
        len(&v["ledger"]["requests"], self.reads)
            && v["ledger"]["completed"] == self.reads - usize::from(self.loading && self.read_held)
            && v["ledger"]["held"] == self.read_held
            && v["ledger"]["cancelled"] == self.cancelled
            && v["comments"]["loading"] == self.loading
            && len(&v["reactionLedger"]["prepared"], self.prepared)
            && len(&v["reactionLedger"]["issued"], self.prepared)
            && v["reactionLedger"]["completed"] == self.completed
            && v["reactionLedger"]["held"] == self.held
            && len(&v["reactionLedger"]["fullReads"], self.full_reads)
            && v["reactionActions"]["attempts"]
                .as_array()
                .is_some_and(|a| {
                    a.len() == self.phases.len()
                        && a.iter().zip(&self.phases).all(|(a, p)| a["phase"] == *p)
                })
            && v["familyProbe"]
                .as_array()
                .and_then(|a| a.iter().find(|r| r["name"] == family::SELECTED))
                .is_some_and(|r| r["available"] == !self.disabled)
    }
}
async fn wait(state: Signal<ShellState>, m: &Model, e: &Expected) -> Result<Value, String> {
    for _ in 0..250 {
        let v = snapshot(state, m)?;
        if e.ready(&v) {
            return Ok(v);
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    Err(format!("Family did not settle: {}", snapshot(state, m)?))
}
async fn capture(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    name: &str,
    p: &Value,
    rows: &mut Vec<Value>,
    e: Expected,
) -> Result<(), String> {
    wait(state, m, &e).await?;
    stable_drafts(state, m).await?;
    checked(d,"(()=>{const r=document.querySelector('[data-list-comment-ts=\"1781524801.000001\"] .message-actions');if(!r)return {ok:false};r.scrollIntoView({block:'nearest'});return {ok:true};})()").await?;
    let before = snapshot(state, m)?;
    let expected = family::reactions(&e.removed);
    let probe = before["familyProbe"]
        .as_array()
        .ok_or("Family probe missing")?;
    let expected = expected.as_array().unwrap();
    if probe.len() != expected.len()
        || probe.iter().zip(expected).any(|(a, b)| {
            a["name"] != b["name"]
                || a["count"] != b["count"]
                || a["own"] != b["users"].as_array().unwrap().contains(&json!("U0"))
        })
    {
        return Err("Family changed alias/nonmember membership".into());
    }
    let expression = format!(
        "({})(({}),{},{})",
        include_str!("reaction_family.js"),
        include_str!("reaction_actions/text.js"),
        before["familyProbe"],
        before["familyStatus"]
    );
    let geometry = wait_dom(d, &expression).await?;
    let mut proof = p.clone();
    proof["journal"] = compose::journal_snapshot(state).await?;
    if !e.ready(&before)
        || before["noSession"] != true
        || before["noTransport"] != true
        || before["writes"] != 0
        || before["networkCalls"] != 0
        || !counts(&before, 0, 0)
        || geometry["editor"]["blocks"] != proof["originalA"]
        || proof["journal"]["blocks"] != proof["originalA"]
    {
        return Err("Family lost source/draft custody".into());
    }
    let capture = harness::capture(d, name).await?;
    let after = snapshot(state, m)?;
    let journal_after = compose::journal_snapshot(state).await?;
    if !stability::unchanged(name, &before, &after, &proof["journal"], &journal_after) {
        return Err(format!("Family changed during {name} capture"));
    }
    rows.push(
        json!({"stage":name,"model":before,"geometry":geometry,"proof":proof,"capture":capture}),
    );
    Ok(())
}
async fn click_family(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    p: &mut Value,
    stage: &str,
    modes: &[dispatch::Mode],
    disabled: bool,
) -> Result<(), String> {
    if !modes.is_empty() {
        dispatch::sequence(&state.peek(), m, modes)?;
    }
    let before = snapshot(state, m)?;
    let input=checked(d,&format!("(()=>{{const b=document.querySelector('[data-list-comment-ts=\"1781524801.000001\"] [data-reaction-name=\"{}\"]');if(!b||b.disabled!=={})return {{ok:false}};b.scrollIntoView({{block:'nearest'}});const v={{ok:true,name:b.dataset.reactionName,disabled:b.disabled,pressed:b.getAttribute('aria-pressed')}};b.click();return v;}})()",family::SELECTED,disabled)).await?;
    if disabled {
        tokio::time::sleep(Duration::from_millis(120)).await;
        if snapshot(state, m)? != before {
            return Err("Disabled family click mutated state".into());
        }
    }
    p["inputs"]
        .as_array_mut()
        .unwrap()
        .push(json!({"stage":stage,"value":input}));
    Ok(())
}
fn release(m: &Model, p: &mut Value, stage: &str) -> Result<(), String> {
    let v = dispatch::release(m)?;
    if v != json!({"delivered":true,"cancelledBeforeReply":false}) {
        return Err("Family original ACK not delivered".into());
    }
    p["releases"]
        .as_array_mut()
        .unwrap()
        .push(json!({"stage":stage,"value":v}));
    Ok(())
}
pub(super) async fn run(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    case: &str,
) -> Result<Value, String> {
    let (readiness, m, mut p) = edits::setup(state, d).await?;
    p["initialResponse"] = snapshot(state, &m)?["comments"]["page"].clone();
    p["inputs"] = json!([]);
    p["releases"] = json!([]);
    p["socket"] = Value::Null;
    p["serverUpdates"] = json!([]);
    let mut rows = vec![];
    if case == "reaction-family" {
        success::run(state, &m, d, &mut p, &mut rows).await?;
    } else {
        failures::run(state, &m, d, &mut p, &mut rows).await?;
    }
    Ok(
        json!({"ok":true,"case":case,"fixture":readiness["fixture"],"readiness":readiness,"stages":rows,"final":snapshot(state,&m)?,"initialRecords":fixture::initial_records(&m),"noLiveNetwork":true}),
    )
}
