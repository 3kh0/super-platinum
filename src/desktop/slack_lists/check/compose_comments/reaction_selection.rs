//! Local callback picker exercised through original DOM buttons only.
use super::*;
use crate::slack_lists::comments::{
    fixture::{reaction_actions as dispatch, reaction_selection as data},
    reaction_actions::{self as actions, selection},
};
mod custody;
mod failures;
#[path = "reaction_actions/stability.rs"]
mod stability;
mod success;
fn snapshot(state: Signal<ShellState>, m: &Model) -> Result<Value, String> {
    let s = state.peek();
    let mut v = compose::snapshot(&s, m);
    let view =
        crate::slack_lists::comments::presentation(&s).ok_or("Selection comments missing")?;
    let row = view
        .rows
        .iter()
        .find(|r| r.ts == data::FIRST)
        .ok_or("Selection reply missing")?;
    v["reactionActions"] = actions::snapshot(&s);
    v["picker"] = selection::snapshot(&s);
    v["selectionSource"] = data::observed(&s, m)?;
    v["selectionProbe"] = json!(
        row.reactions
            .iter()
            .map(|r| json!({"name":r.name,"count":r.count,"own":r.own,"glyph":r.glyph}))
            .collect::<Vec<_>>()
    );
    v["actionProbe"] = json!({"available":selection::available(&s,view.generation,data::FIRST),"status":actions::status(&s,view.generation)});
    Ok(v)
}
struct Expected {
    reads: usize,
    posts: usize,
    completed: usize,
    phases: Vec<&'static str>,
    picker: Option<bool>,
    reactions: Value,
}
impl Expected {
    fn ready(&self, v: &Value) -> bool {
        let len = |a: &Value, n| a.as_array().is_some_and(|a| a.len() == n);
        len(&v["ledger"]["requests"], self.reads)
            && v["ledger"]["completed"] == self.reads
            && v["ledger"]["cancelled"] == 0
            && v["ledger"]["held"] == false
            && len(&v["reactionLedger"]["prepared"], self.posts)
            && len(&v["reactionLedger"]["issued"], self.posts)
            && v["reactionLedger"]["completed"] == self.completed
            && v["reactionLedger"]["held"] == (self.posts > self.completed)
            && v["reactionActions"]["attempts"]
                .as_array()
                .is_some_and(|a| {
                    a.len() == self.phases.len()
                        && a.iter().zip(&self.phases).all(|(a, p)| a["phase"] == *p)
                })
            && match self.picker {
                Some(p) => v["picker"]["ready"] == p,
                None => v["picker"].is_null(),
            }
    }
}
async fn wait(
    state: Signal<ShellState>,
    m: &Model,
    test: impl Fn(&Value) -> bool,
) -> Result<Value, String> {
    for _ in 0..250 {
        let v = snapshot(state, m)?;
        if test(&v) {
            return Ok(v);
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    Err(format!("Selection did not settle: {}", snapshot(state, m)?))
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
    wait(state, m, |v| e.ready(v)).await?;
    stable_drafts(state, m).await?;
    if e.picker.is_none() {
        checked(d,"(()=>{const e=document.querySelector('[data-list-comment-ts=\"1781524801.000001\"] .message-actions');if(!e)return {ok:false};e.scrollIntoView({block:'nearest'});return {ok:true};})()").await?;
    }
    let before = snapshot(state, m)?;
    let expected = e
        .reactions
        .as_array()
        .ok_or("Expected reaction rows missing")?;
    if !before["selectionProbe"].as_array().is_some_and(|a| {
        a.len() == expected.len()
            && a.iter().zip(expected).all(|(a, b)| {
                a["name"] == b["name"]
                    && a["count"] == b["count"]
                    && a["own"] == b["users"].as_array().unwrap().contains(&json!("U0"))
            })
    }) {
        return Err("Exact selection changed unexpected siblings".into());
    }
    let expression = format!(
        "({})(({}),{},{},{})",
        include_str!("reaction_selection.js"),
        include_str!("reaction_actions/text.js"),
        before["selectionProbe"],
        json!(e.picker),
        before["actionProbe"]["status"]
    );
    let geometry = wait_dom(d, &expression).await?;
    let mut proof = p.clone();
    proof["journal"] = compose::journal_snapshot(state).await?;
    if !e.ready(&before)
        || before["noSession"] != true
        || before["noTransport"] != true
        || before["networkCalls"] != 0
        || before["writes"] != 0
        || !counts(&before, 0, 0)
        || geometry["editor"]["blocks"] != proof["originalA"]
        || proof["journal"]["blocks"] != proof["originalA"]
    {
        return Err("Selection lost closed source or draft custody".into());
    }
    let capture = harness::capture(d, name).await?;
    let after = snapshot(state, m)?;
    let journal = compose::journal_snapshot(state).await?;
    if !stability::unchanged(name, &before, &after, &proof["journal"], &journal) {
        return Err(format!("Selection changed during {name} capture"));
    }
    rows.push(
        json!({"stage":name,"model":before,"geometry":geometry,"proof":proof,"capture":capture}),
    );
    Ok(())
}
async fn open_picker(d: &dioxus::desktop::DesktopContext) -> Result<Value, String> {
    let input=checked(d,"(()=>{const b=document.querySelector('[data-list-comment-ts=\"1781524801.000001\"] [data-list-reaction-add]');if(!b||b.disabled)return {ok:false};b.scrollIntoView({block:'nearest'});b.click();return {ok:true,ts:b.dataset.listReactionAdd};})()").await?;
    wait_dom(
        d,
        "({ok:!!document.querySelector('#slack-list-reaction-picker #emoji-query')})",
    )
    .await?;
    Ok(input)
}
async fn search(d: &dioxus::desktop::DesktopContext, query: &str) -> Result<Value, String> {
    checked(d,&format!("(()=>{{const q=document.querySelector('#slack-list-reaction-picker #emoji-query');if(!q)return {{ok:false}};Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(q,{});q.dispatchEvent(new Event('input',{{bubbles:true}}));return {{ok:true,query:q.value}};}})()",json!(query))).await?;
    wait_dom(d,&format!("(()=>{{const expected={},q=document.querySelector('#slack-list-reaction-picker #emoji-query'),a=[...document.querySelectorAll('#slack-list-reaction-picker [role=option]')],names=a.map(b=>b.getAttribute('aria-label'));return {{ok:q?.value===expected&&names.includes(expected)&&(expected!=='+1'||names.length===1),query:q?.value,names}};}})()",json!(query))).await
}
async fn close_picker(d: &dioxus::desktop::DesktopContext) -> Result<Value, String> {
    let v=checked(d,"(()=>{const b=document.querySelector('#slack-list-reaction-picker [aria-label=Close]');if(!b)return {ok:false};b.click();return {ok:true};})()").await?;
    wait_dom(
        d,
        "({ok:!document.getElementById('slack-list-reaction-picker')})",
    )
    .await?;
    Ok(v)
}
async fn pick(
    d: &dioxus::desktop::DesktopContext,
    name: &str,
    disabled: bool,
) -> Result<Value, String> {
    wait_dom(d,&format!("(()=>{{const b=[...document.querySelectorAll('#slack-list-reaction-picker [role=option]')].find(b=>b.getAttribute('aria-label')==={});return {{ok:!!b&&b.disabled==={}}};}})()",json!(name),disabled)).await?;
    checked(d,&format!("(()=>{{const b=[...document.querySelectorAll('#slack-list-reaction-picker [role=option]')].find(b=>b.getAttribute('aria-label')==={});if(!b||b.disabled!=={})return {{ok:false}};b.scrollIntoView({{block:'nearest'}});const r=b.getBoundingClientRect(),h=document.elementFromPoint((r.left+r.right)/2,(r.top+r.bottom)/2);if(h!==b&&!b.contains(h))return {{ok:false}};const v={{ok:true,name:b.getAttribute('aria-label'),disabled:b.disabled,token:document.getElementById('slack-list-reaction-picker').dataset.reactionPicker}};b.click();return v;}})()",json!(name),disabled)).await
}
async fn choose(
    state: Signal<ShellState>,
    m: &Model,
    d: &dioxus::desktop::DesktopContext,
    p: &mut Value,
    stage: &str,
    name: &str,
    mode: dispatch::Mode,
) -> Result<(), String> {
    dispatch::mode(&state.peek(), m, mode)?;
    open_picker(d).await?;
    wait(state, m, |v| v["picker"]["ready"] == true).await?;
    search(d, name).await?;
    let input = pick(d, name, false).await?;
    p["inputs"]
        .as_array_mut()
        .unwrap()
        .push(json!({"stage":stage,"value":input}));
    Ok(())
}
fn future(
    state: Signal<ShellState>,
    m: &Model,
    p: &mut Value,
    stage: &str,
    reactions: Value,
) -> Result<(), String> {
    let raw = data::server(&state.peek(), m, reactions)?;
    p["serverUpdates"]
        .as_array_mut()
        .unwrap()
        .push(json!({"stage":stage,"raw":raw}));
    Ok(())
}
fn release(m: &Model, p: &mut Value, stage: &str) -> Result<(), String> {
    let v = dispatch::release(m)?;
    if v != json!({"delivered":true,"cancelledBeforeReply":false}) {
        return Err("Original reaction reply not delivered".into());
    }
    p["releases"]
        .as_array_mut()
        .unwrap()
        .push(json!({"stage":stage,"value":v}));
    Ok(())
}
fn with(base: &Value, name: &str, own: bool) -> Value {
    let mut rows = base.as_array().unwrap().clone();
    rows.retain(|r| r["name"] != name);
    if own {
        rows.push(json!({"name":name,"count":1,"users":["U0"]}));
    }
    json!(rows)
}
pub(super) async fn run(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    case: &str,
) -> Result<Value, String> {
    let (readiness, m, mut p) = edits::setup(state, d).await?;
    p["initialResponse"] = snapshot(state, &m)?["comments"]["page"].clone();
    for key in [
        "inputs",
        "releases",
        "serverUpdates",
        "sourceChanges",
        "socket",
    ] {
        p[key] = json!([]);
    }
    let mut rows = vec![];
    match case {
        "reaction-selection" => success::run(state, &m, d, &mut p, &mut rows).await?,
        "reaction-selection-failures" => failures::run(state, &m, d, &mut p, &mut rows).await?,
        _ => custody::run(state, &m, d, &mut p, &mut rows).await?,
    };
    Ok(
        json!({"ok":true,"case":case,"fixture":readiness["fixture"],"readiness":readiness,"stages":rows,"final":snapshot(state,&m)?,"initialRecords":fixture::initial_records(&m),"noLiveNetwork":true}),
    )
}
