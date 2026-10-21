//! Actual existing-pill clicks, including cold observed emoji acquisition.
use super::*;
use crate::slack_lists::comments::{
    fixture::{reaction_actions as dispatch, reaction_additions as data},
    reaction_actions::{self as actions, addition, selection},
};
mod custody;
#[path = "reaction_actions/stability.rs"]
mod stability;
mod success;
fn snapshot(state: Signal<ShellState>, m: &Model) -> Result<Value, String> {
    let s = state.peek();
    let mut v = compose::snapshot(&s, m);
    let view =
        crate::slack_lists::comments::presentation(&s).ok_or("Additions comments missing")?;
    let row = view
        .rows
        .iter()
        .find(|r| r.ts == data::FIRST)
        .ok_or("Additions row missing")?;
    v["reactionActions"] = actions::snapshot(&s);
    v["addition"] = addition::snapshot(&s);
    v["picker"] = selection::snapshot(&s);
    v["additionSource"] = data::observed(&s, m)?;
    v["additionProbe"]=json!(row.reactions.iter().map(|r|json!({"name":r.name,"count":r.count,"own":r.own,"glyph":r.glyph,"available":actions::available(&s,view.generation,data::FIRST,&r.name)})).collect::<Vec<_>>());
    v["actionProbe"] = json!({"status":actions::status(&s,view.generation)});
    Ok(v)
}
struct Expected {
    reads: usize,
    posts: usize,
    phases: Vec<&'static str>,
    addition: Option<&'static str>,
    prefs: usize,
    pref_completed: u64,
    pref_held: bool,
    preflights: usize,
    preflight_held: bool,
    reactions: Value,
    target: &'static str,
}
impl Expected {
    fn ready(&self, v: &Value) -> bool {
        let len = |a: &Value, n| a.as_array().is_some_and(|a| a.len() == n);
        len(&v["ledger"]["requests"], self.reads)
            && v["ledger"]["completed"] == self.reads
            && v["ledger"]["cancelled"] == 0
            && len(&v["reactionLedger"]["prepared"], self.posts)
            && len(&v["reactionLedger"]["issued"], self.posts)
            && v["reactionLedger"]["completed"] == self.posts
            && v["reactionActions"]["attempts"]
                .as_array()
                .is_some_and(|a| {
                    a.len() == self.phases.len()
                        && a.iter().zip(&self.phases).all(|(a, p)| a["phase"] == *p)
                })
            && match self.addition {
                Some(p) => v["addition"]["phase"] == p,
                None => v["addition"].is_null(),
            }
            && v["picker"].is_null()
            && len(&v["additionSource"]["requests"], self.prefs)
            && v["additionSource"]["completed"] == self.pref_completed
            && v["additionSource"]["held"] == self.pref_held
            && preflight_ready(v, self.preflights, self.preflight_held)
    }
}
fn preflight_ready(v: &Value, count: usize, held: bool) -> bool {
    v["composeLedger"]["preflights"]
        .as_array()
        .is_some_and(|rows| rows.len() == count)
        && v["composeLedger"]["held"] == held
        && v["comments"]["loading"] == held
}
async fn wait(
    state: Signal<ShellState>,
    m: &Model,
    test: impl Fn(&Value) -> bool,
) -> Result<Value, String> {
    for _ in 0..275 {
        let v = snapshot(state, m)?;
        if test(&v) {
            return Ok(v);
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    Err(format!(
        "Pill addition did not settle: {}",
        snapshot(state, m)?
    ))
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
    let before = snapshot(state, m)?;
    let expected = e.reactions.as_array().ok_or("Reaction oracle missing")?;
    if !before["additionProbe"].as_array().is_some_and(|a| {
        a.len() == expected.len()
            && a.iter().zip(expected).all(|(a, b)| {
                a["name"] == b["name"]
                    && a["count"] == b["count"]
                    && a["own"] == b["users"].as_array().unwrap().contains(&json!("U0"))
            })
    }) {
        return Err("Pill addition changed exact raw siblings".into());
    }
    let mut proof = p.clone();
    let mut views = vec![];
    for raw in expected {
        views.push(pill_view(d, raw["name"].as_str().unwrap()).await?);
    }
    pill_view(d, e.target).await?;
    proof["pillViews"] = json!(views);
    proof["journal"] = compose::journal_snapshot(state).await?;
    let expression = format!(
        "({})(({}),{},{},{})",
        include_str!("reaction_additions.js"),
        include_str!("reaction_actions/text.js"),
        before["additionProbe"],
        json!(e.target),
        before["actionProbe"]["status"]
    );
    let geometry = wait_dom(d, &expression).await?;
    if !e.ready(&before)
        || before["noSession"] != true
        || before["noTransport"] != true
        || before["networkCalls"] != 0
        || before["writes"] != 0
        || !counts(&before, 0, 0)
        || geometry["editor"]["blocks"] != proof["originalA"]
        || proof["journal"]["blocks"] != proof["originalA"]
    {
        return Err("Pill addition lost draft or closed ownership".into());
    }
    let capture = harness::capture(d, name).await?;
    let after = snapshot(state, m)?;
    let journal = compose::journal_snapshot(state).await?;
    if !stability::unchanged(name, &before, &after, &proof["journal"], &journal) {
        return Err(format!(
            "Addition changed during {name} capture: {}",
            json!({"before":before,"after":after,"journalBefore":proof["journal"],"journalAfter":journal})
        ));
    }
    rows.push(
        json!({"stage":name,"model":before,"geometry":geometry,"proof":proof,"capture":capture}),
    );
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn completed_reads_do_not_establish_held_generic_info_admission() {
        let mut v = json!({"ledger":{"completed":8},"reactionActions":{"attempts":[{"phase":"preparing"}]},"composeLedger":{"preflights":[],"held":false},"comments":{"loading":true}});
        assert!(!preflight_ready(&v, 1, true));
        v["composeLedger"]["preflights"] = json!([{"method":"conversations.genericInfo"}]);
        assert!(!preflight_ready(&v, 1, true));
        v["composeLedger"]["held"] = json!(true);
        assert!(preflight_ready(&v, 1, true));
        assert!(!preflight_ready(&v, 2, true));
        v["comments"]["loading"] = json!(false);
        assert!(!preflight_ready(&v, 1, true));
    }
    #[test]
    fn terminal_capture_requires_released_preflight_and_finished_loading() {
        let mut v =
            json!({"composeLedger":{"preflights":[{}],"held":true},"comments":{"loading":true}});
        assert!(!preflight_ready(&v, 1, false));
        v["composeLedger"]["held"] = json!(false);
        assert!(!preflight_ready(&v, 1, false));
        v["comments"]["loading"] = json!(false);
        assert!(preflight_ready(&v, 1, false));
    }
}
async fn pill_view(d: &dioxus::desktop::DesktopContext, name: &str) -> Result<Value, String> {
    wait_dom(d,&format!(r#"(()=>{{const name={},r=document.querySelector('[data-list-comment-ts="1781524801.000001"]'),b=[...(r?.querySelectorAll('button[data-reaction-name]')||[])].find(b=>b.dataset.reactionName===name);if(!b)return {{ok:false}};b.scrollIntoView({{block:'nearest',inline:'nearest'}});const x=b.getBoundingClientRect(),p=document.elementFromPoint((x.left+x.right)/2,(x.top+x.bottom)/2),hit=p===b||b.contains(p);return {{ok:hit&&x.left>=0&&x.right<=innerWidth+1&&x.top>=0&&x.bottom<=innerHeight+1,name,hit,rect:{{left:x.left,top:x.top,right:x.right,bottom:x.bottom,width:x.width,height:x.height}}}};}})()"#,json!(name))).await
}
async fn click(
    d: &dioxus::desktop::DesktopContext,
    p: &mut Value,
    stage: &str,
    name: &str,
) -> Result<(), String> {
    pill_view(d, name).await?;
    let input=checked(d,&format!(r#"(()=>{{const name={},r=document.querySelector('[data-list-comment-ts="1781524801.000001"]'),b=[...(r?.querySelectorAll('button[data-reaction-name]')||[])].find(b=>b.dataset.reactionName===name);if(!b||b.disabled||document.getElementById('slack-list-reaction-picker'))return {{ok:false}};b.click();return {{ok:true,name,picker:false}};}})()"#,json!(name))).await?;
    p["inputs"]
        .as_array_mut()
        .unwrap()
        .push(json!({"stage":stage,"value":input}));
    Ok(())
}
fn own(base: &Value, name: &str) -> Value {
    let mut a = base.as_array().unwrap().clone();
    if let Some(r) = a.iter_mut().find(|r| r["name"] == name) {
        r["count"] = json!(2);
        r["users"] = json!(["U1", "U0"])
    } else {
        a.push(json!({"name":name,"count":1,"users":["U0"]}));
    }
    json!(a)
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
fn prefs(
    mut state: Signal<ShellState>,
    m: &Model,
    p: &mut Value,
    stage: &str,
    value: i32,
) -> Result<(), String> {
    let before = data::observed(&state.peek(), m)?;
    let frame = json!({"type":"pref_change","name":"preferred_skin_tone","value":value});
    let event = super_platinum_core::slack::realtime::parse_event(&frame.to_string())
        .ok_or("Preference event missing")?;
    crate::realtime::apply_in_view(&mut state.write().core, "T1", 7, event, false);
    let after = data::observed(&state.peek(), m)?;
    if before["preferences"] == after["preferences"] {
        return Err("Actual prefs event did not apply".into());
    }
    p["sourceChanges"].as_array_mut().unwrap().push(json!({"stage":stage,"kind":"pref_change","team":"T1","generation":7,"frame":frame,"before":before,"after":after}));
    Ok(())
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
    if case == "pill-additions" {
        success::run(state, &m, d, &mut p, &mut rows).await?
    } else {
        custody::run(state, &m, d, &mut p, &mut rows).await?
    }
    Ok(
        json!({"ok":true,"case":case,"fixture":readiness["fixture"],"readiness":readiness,"stages":rows,"final":snapshot(state,&m)?,"initialRecords":fixture::initial_records(&m),"noLiveNetwork":true}),
    )
}
