//! Native existing-pill actions retain the mounted rich draft through original receipts.
use super::*;
use crate::slack_lists::comments::{
    fixture::reaction_actions as dispatch, reaction_actions as actions,
};
mod failures;
mod stability;
mod success;
const TS: &str = "1781524801.000001";
const CHANNEL: &str = "CCOMMENT01";
fn snapshot(state: Signal<ShellState>, model: &Model) -> Value {
    let shell = state.peek();
    let mut raw = compose::snapshot(&shell, model);
    raw["reactionActions"] = actions::snapshot(&shell);
    let view = crate::slack_lists::comments::presentation(&shell);
    raw["actionProbe"]=view.map(|view|{let selected=view.rows.iter().find(|r|r.ts==TS).and_then(|m|m.reactions.iter().find(|r|r.name=="wave"));
        json!({"ts":TS,"name":"wave","generation":view.generation,"own":selected.is_some_and(|r|r.own),"count":selected.map_or(0,|r|r.count),"available":actions::available(&shell,view.generation,TS,"wave"),"status":actions::status(&shell,view.generation)})}).unwrap_or(Value::Null);
    raw
}
async fn wait(
    state: Signal<ShellState>,
    model: &Model,
    check: impl Fn(&Value) -> bool,
) -> Result<Value, String> {
    for _ in 0..250 {
        let v = snapshot(state, model);
        if check(&v) {
            return Ok(v);
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    Err(format!(
        "Reaction action did not settle: {}",
        snapshot(state, model)
    ))
}
#[derive(Clone, Copy)]
struct Expected {
    reads: u64,
    prepared: u64,
    issued: u64,
    completed: u64,
    attempts: u64,
    held: bool,
    own: bool,
    count: u64,
    disabled: bool,
    phase: Option<&'static str>,
    read_held: bool,
    loading: bool,
    cancelled: u64,
}
impl Expected {
    fn ready(self, v: &Value) -> bool {
        v["ledger"]["requests"]
            .as_array()
            .is_some_and(|a| a.len() as u64 == self.reads)
            && v["ledger"]["completed"] == self.reads - u64::from(self.read_held && self.loading)
            && v["ledger"]["held"] == self.read_held
            && v["ledger"]["cancelled"] == self.cancelled
            && v["comments"]["loading"] == self.loading
            && v["reactionLedger"]["prepared"]
                .as_array()
                .is_some_and(|a| a.len() as u64 == self.prepared)
            && v["reactionLedger"]["issued"]
                .as_array()
                .is_some_and(|a| a.len() as u64 == self.issued)
            && v["reactionLedger"]["completed"] == self.completed
            && v["reactionLedger"]["held"] == self.held
            && v["reactionActions"]["attempts"]
                .as_array()
                .is_some_and(|a| {
                    a.len() as u64 == self.attempts
                        && match self.phase {
                            Some(p) => a.last().is_some_and(|a| a["phase"] == p),
                            None => a.is_empty(),
                        }
                })
            && v["actionProbe"]["own"] == self.own
            && v["actionProbe"]["count"] == self.count
            && v["actionProbe"]["available"] == !self.disabled
    }
}
async fn capture(
    state: Signal<ShellState>,
    model: &Model,
    d: &dioxus::desktop::DesktopContext,
    name: &str,
    proof: &Value,
    rows: &mut Vec<Value>,
    expected: Expected,
) -> Result<(), String> {
    wait(state, model, |v| expected.ready(v)).await?;
    stable_drafts(state, model).await?;
    checked(d,"(()=>{const r=document.querySelector('[data-list-comment-ts=\"1781524801.000001\"]');if(!r)return {ok:false};(r.querySelector('[data-reaction-name=wave]')||r.querySelector('.message-body')).scrollIntoView({block:'nearest'});return {ok:true};})()").await?;
    let before = snapshot(state, model);
    let status = before["actionProbe"]["status"].clone();
    let expression = format!(
        r#"(()=>{{const v=window.__listCompose.snapshot(),row=document.querySelector('[data-list-comment-ts="1781524801.000001"]'),b=row?.querySelector('[data-reaction-name=wave]'),s=document.getElementById('slack-list-comments-reaction-status'),scroll=document.querySelector('.slack-list-comments-scroll');const rect=e=>{{const r=e.getBoundingClientRect();return {{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}};}};const visible=e=>{{if(!e||e.closest('[hidden]'))return false;const r=rect(e),cs=getComputedStyle(e);return cs.display!=='none'&&cs.visibility==='visible'&&r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1;}};const hit=e=>{{if(!visible(e))return false;const r=rect(e),p=document.elementFromPoint((r.left+r.right)/2,(r.top+r.bottom)/2);return p===e||e.contains(p);}};v.action={{editorText:({text_probe})(document.getElementById('slack-list-comment-draft')),button:b?{{tag:b.tagName,type:b.type,name:b.dataset.reactionName,disabled:b.disabled,ariaDisabled:b.getAttribute('aria-disabled'),pressed:b.getAttribute('aria-pressed'),own:b.classList.contains('own'),count:Number(b.querySelector('.reaction-count')?.textContent),visible:visible(b),hit:hit(b),rect:rect(b)}}:null,status:s?{{text:s.textContent,role:s.getAttribute('role'),visible:visible(s),outsideScroll:!scroll?.contains(s),rect:rect(s)}}:null,tools:row?.querySelectorAll('.message-tools').length||0}};v.ok=v.ok&&v.action.editorText.ok&&({count}===0?!b:!!b&&b.disabled==={disabled}&&b.getAttribute('aria-pressed')==={pressed})&&v.editor?.sameNode===true&&v.editor?.editable===true&&({status}===null?!s:s?.textContent==={status});return v;}})()"#,
        text_probe = include_str!("reaction_actions/text.js"),
        count = expected.count,
        disabled = expected.disabled,
        pressed = json!(expected.own.to_string()),
        status = status
    );
    let geometry = wait_dom(d, &expression).await?;
    let mut proof = proof.clone();
    proof["journal"] = compose::journal_snapshot(state).await?;
    let button = &geometry["action"]["button"];
    if !expected.ready(&before)
        || before["noSession"] != true
        || before["noTransport"] != true
        || before["networkCalls"] != 0
        || before["writes"] != 0
        || !counts(&before, 0, 0)
        || geometry["editor"]["blocks"] != proof["originalA"]
        || proof["journal"]["blocks"] != proof["originalA"]
        || (expected.count > 0
            && (button["tag"] != "BUTTON"
                || button["type"] != "button"
                || button["visible"] != true
                || button["hit"] != true
                || button["ariaDisabled"] != expected.disabled.to_string()
                || button["own"] != expected.own
                || button["count"] != expected.count))
        || (expected.count == 0 && !button.is_null())
        || geometry["action"]["editorText"]["ok"] != true
        || geometry["action"]["tools"] != 0
        || (!status.is_null()
            && (geometry["action"]["status"]["visible"] != true
                || geometry["action"]["status"]["outsideScroll"] != true
                || geometry["action"]["status"]["role"] != "status"))
    {
        return Err(format!(
            "Reaction stage {name} lost source, draft or accessible pill/status geometry"
        ));
    }
    let capture = harness::capture(d, name).await?;
    let after = snapshot(state, model);
    let journal_after = compose::journal_snapshot(state).await?;
    if !stability::unchanged(name, &before, &after, &proof["journal"], &journal_after) {
        return Err(format!(
            "Reaction changed during {name} capture: {}",
            capture_difference(&before, &after, &proof["journal"], &journal_after)
        ));
    }
    rows.push(
        json!({"stage":name,"model":before,"geometry":geometry,"proof":proof,"capture":capture}),
    );
    Ok(())
}
fn capture_difference(
    before: &Value,
    after: &Value,
    journal: &Value,
    journal_after: &Value,
) -> Value {
    fn paths(a: &Value, b: &Value, prefix: String, result: &mut Vec<String>) {
        if a == b || result.len() >= 32 {
            return;
        }
        match (a, b) {
            (Value::Object(a), Value::Object(b)) => {
                let keys = a
                    .keys()
                    .chain(b.keys())
                    .collect::<std::collections::BTreeSet<_>>();
                for key in keys {
                    paths(
                        a.get(key).unwrap_or(&Value::Null),
                        b.get(key).unwrap_or(&Value::Null),
                        format!("{prefix}/{key}"),
                        result,
                    );
                }
            }
            (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
                for (index, (a, b)) in a.iter().zip(b).enumerate() {
                    paths(a, b, format!("{prefix}/{index}"), result);
                }
            }
            _ => result.push(prefix),
        }
    }
    fn timers(v: &Value) -> Value {
        json!({"read":v["comments"]["read"],"retryAt":v["comments"]["retryAt"],"retryAfter":v["comments"]["retryAfter"],"actionProbe":v["actionProbe"]})
    }
    let mut changed = Vec::new();
    paths(before, after, "model".into(), &mut changed);
    paths(journal, journal_after, "journal".into(), &mut changed);
    json!({"modelChanged":before!=after,"journalChanged":journal!=journal_after,"changedPaths":changed,"before":timers(before),"after":timers(after)})
}
async fn click_pill(d: &dioxus::desktop::DesktopContext, disabled: bool) -> Result<Value, String> {
    checked(d,&format!(r#"(()=>{{const b=document.querySelector('[data-list-comment-ts="1781524801.000001"] button[data-reaction-name=wave]');if(!b||b.disabled!=={disabled})return {{ok:false}};b.scrollIntoView({{block:'nearest'}});b.focus();const v={{ok:true,disabled:b.disabled,pressed:b.getAttribute('aria-pressed'),type:b.type,focused:document.activeElement===b}};b.click();return v;}})()"#)).await
}
fn socket(
    state: Signal<ShellState>,
    model: &Model,
    stage: &str,
    added: bool,
) -> Result<Value, String> {
    let user = compose::session(&state.peek())
        .map_err(|_| "Self missing")?
        .user_id;
    socket_user(state, model, stage, added, &user)
}
fn socket_user(
    mut state: Signal<ShellState>,
    model: &Model,
    stage: &str,
    added: bool,
    user: &str,
) -> Result<Value, String> {
    let frame = json!({"type":if added{"reaction_added"}else{"reaction_removed"},"user":user,"reaction":"wave","item":{"type":"message","channel":CHANNEL,"ts":TS}});
    let event = super_platinum_core::slack::realtime::parse_event(&frame.to_string())
        .ok_or("Reaction parsing failed")?;
    let before = snapshot(state, model);
    if crate::slack_lists::events::prepare(&mut state.write(), "T1", 7, &event) {
        return Err("Reaction requested List refresh".into());
    }
    let after = snapshot(state, model);
    if before["comments"]["page"] != after["comments"]["page"]
        || before["draftStore"] != after["draftStore"]
        || before["actionProbe"]["count"] == after["actionProbe"]["count"]
    {
        return Err("Parsed reaction changed wrong source".into());
    }
    Ok(
        json!({"stage":stage,"team":"T1","generation":7,"frame":frame,"parsed":true,"before":before["actionProbe"],"after":after["actionProbe"]}),
    )
}
fn server(
    state: Signal<ShellState>,
    model: &Model,
    proof: &mut Value,
    stage: &str,
    own: bool,
) -> Result<(), String> {
    let raw = dispatch::server(&state.peek(), model, own)?;
    proof["serverUpdates"]
        .as_array_mut()
        .unwrap()
        .push(json!({"stage":stage,"value":raw}));
    Ok(())
}
fn release(model: &Model, proof: &mut Value, stage: &str) -> Result<(), String> {
    let value = dispatch::release(model)?;
    if value != json!({"delivered":true,"cancelledBeforeReply":false}) {
        return Err("Original reaction receipt not delivered".into());
    }
    proof["releases"]
        .as_array_mut()
        .unwrap()
        .push(json!({"stage":stage,"value":value}));
    Ok(())
}
pub(super) async fn run(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
    case: &str,
) -> Result<Value, String> {
    let (readiness, model, mut proof) = edits::setup(state, d).await?;
    proof["serverUpdates"] = json!([]);
    proof["releases"] = json!([]);
    proof["inputs"] = json!([]);
    let (mut rows, mut events) = (vec![], vec![]);
    if case == "reaction-actions" {
        success::run(state, &model, d, &mut proof, &mut rows, &mut events).await?;
    } else {
        failures::run(state, &model, d, &mut proof, &mut rows).await?;
    }
    Ok(
        json!({"ok":true,"case":case,"fixture":readiness["fixture"],"readiness":readiness,"stages":rows,"reactionEvents":events,"final":snapshot(state,&model),"initialRecords":fixture::initial_records(&model),"noLiveNetwork":true}),
    )
}
