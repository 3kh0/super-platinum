//! Loaded original reply edits through parsed frames and the unchanged native editor.
use super::*;
use super_platinum_core::slack::events::RtEvent;
pub(super) const FIRST: &str = "1781524801.000001";
pub(super) const SENT: &str = "1781525001.000001";
pub(super) const CHANNEL: &str = "CCOMMENT01";
pub(super) const NOTICE: &str =
    "Newer comment updates arrived. Loaded comments are retained. Refresh to try again.";
const STAGES: &[&str] = &[
    "draft",
    "rich",
    "plain",
    "cleared",
    "duplicate",
    "held",
    "ignored-held",
    "updated-held",
    "released",
    "unsupported",
    "readback",
];

pub(super) fn blocks(text: &str, bold: bool) -> Value {
    json!([{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":text,"style":if bold{json!({"bold":true})}else{json!({"italic":true})}}]}]}])
}
pub(super) fn changed(ts: &str, text: &str) -> Value {
    json!({"type":"message","subtype":"message_changed","channel":CHANNEL,"message":{"ts":ts,"thread_ts":fixture::ROOT,"user":"U1","text":text}})
}
pub(super) fn event_state(v: &Value) -> Value {
    let mut result = json!({});
    for key in [
        "read",
        "loading",
        "error",
        "page",
        "pageMessages",
        "live",
        "accepted",
        "editCustody",
        "visibleTs",
        "liveTs",
        "acceptedTs",
        "liveReactions",
        "deleted",
        "reactions",
        "reveal",
    ] {
        result[key] = v["comments"][key].clone();
    }
    result
}
pub(super) fn inject(
    mut state: Signal<ShellState>,
    model: &Model,
    name: &str,
    team: &str,
    generation: u64,
    frame: Value,
    expected: bool,
) -> Result<Value, String> {
    let event = super_platinum_core::slack::realtime::parse_event(&frame.to_string())
        .ok_or("Edit frame parsing failed")?;
    let witness = match &event {
        RtEvent::MessageChanged {
            original_update: Some(update),
            ..
        } => {
            json!({"kind":format!("{:?}",update.kind()),"outerChannel":update.outer_channel(),"typedChannel":update.typed_channel(),"typedTs":update.typed_ts(),"raw":update.raw()})
        }
        _ => Value::Null,
    };
    let mut shell = state.write();
    if !compose::enabled(&shell) || shell.core.session.is_some() || shell.core.transport.is_some() {
        return Err("Closed edit fixture required".into());
    }
    let before = compose::snapshot(&shell, model);
    if crate::slack_lists::events::prepare(&mut shell, team, generation, &event) {
        return Err("Edit unexpectedly requested List refresh".into());
    }
    let after = compose::snapshot(&shell, model);
    let actual = before != after;
    if actual != expected
        || before["draftStore"] != after["draftStore"]
        || before["comments"]["page"] != after["comments"]["page"]
    {
        return Err(format!(
            "Edit event {name} changed unexpected source/draft: {} -> {}",
            event_state(&before),
            event_state(&after)
        ));
    }
    Ok(
        json!({"stage":name,"team":team,"generation":generation,"frame":frame,"parsed":true,"witness":witness,"changed":actual,"before":event_state(&before),"after":event_state(&after)}),
    )
}
pub(super) async fn setup(
    mut state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<(Value, Model, Value), String> {
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
    let native=wait_dom(d,"(()=>{const v=window.__listCompose.snapshot();v.ok=v.ok&&v.editor?.bold===true;return v;})()").await?;
    witness(d).await?;
    Ok((
        readiness,
        model,
        json!({"seed":seed,"originalA":native["editor"]["blocks"],"newerB":null,"heldResponse":null,"release":null,"originalMessage":null,"serverResponse":null}),
    ))
}
pub(super) async fn witness(d: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    checked(d,"(()=>{window.__listReactionOriginalEditor=document.getElementById('slack-list-comment-draft');return {ok:!!window.__listReactionOriginalEditor};})()").await?;
    Ok(())
}
pub(super) struct Expected<'a> {
    pub reads: u64,
    pub completed: u64,
    pub cancelled: u64,
    pub held: bool,
    pub loading: bool,
    pub posts: usize,
    pub busy: bool,
    pub same: bool,
    pub ts: &'a str,
    pub text: &'a str,
    pub bold: bool,
    pub italic: bool,
    pub wave: u64,
    pub notice: Option<&'a str>,
    pub blocks: Value,
}
fn ready(v: &Value, e: &Expected<'_>) -> bool {
    v["ledger"]["requests"]
        .as_array()
        .is_some_and(|r| r.len() as u64 == e.reads)
        && v["ledger"]["completed"] == e.completed
        && v["ledger"]["cancelled"] == e.cancelled
        && v["ledger"]["held"] == e.held
        && v["comments"]["loading"] == e.loading
        && counts(v, e.posts, e.posts)
        && v["comments"]["error"] == json!(e.notice)
        && v["draftStore"]["drafts"].as_array().is_some_and(|r| {
            r.len() == 1
                && r[0]["busy"] == e.busy
                && r[0]["saving"] == false
                && r[0]["loading"] == false
                && r[0]["terminalPending"] == false
        })
}
pub(super) async fn capture(
    state: Signal<ShellState>,
    model: &Model,
    d: &dioxus::desktop::DesktopContext,
    name: &str,
    proof: &Value,
    rows: &mut Vec<Value>,
    e: Expected<'_>,
) -> Result<(), String> {
    wait_model(state, model, |v| ready(v, &e)).await?;
    let text = json!(e.text);
    let ts = json!(e.ts);
    let same = e.same;
    let editable = !e.busy;
    let wave = e.wave;
    let notice = e.notice.map_or(String::new(), |n| {
        format!("&&v.notices.includes({})", json!(n))
    });
    let expression = format!(
        "(()=>{{const v=window.__listCompose.snapshot(),r=v.rows.find(r=>r.ts==={ts}),node=[...document.querySelectorAll('[data-list-comment-ts]')].find(r=>r.dataset.listCommentTs==={ts}),b=node?.querySelector('.message-body'),scroll=document.querySelector('.slack-list-comments-scroll');const rect=e=>{{const r=e.getBoundingClientRect();return {{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}};}};const visible=e=>{{if(!e||!scroll)return false;const r=rect(e),s=rect(scroll);return r.width>0&&r.height>0&&r.top>=Math.max(0,s.top)-1&&r.bottom<=Math.min(innerHeight,s.bottom)+1&&r.left>=0&&r.right<=innerWidth+1;}};const strong=b&&[...b.querySelectorAll('.bold,strong,b')].find(e=>e.textContent==={text}),italic=b&&[...b.querySelectorAll('.italic,em,i')].find(e=>e.textContent==={text});v.editBody={{ts:{ts},text:b?.textContent||'',bold:!!strong&&Number(getComputedStyle(strong).fontWeight)>=600,italic:!!italic&&getComputedStyle(italic).fontStyle==='italic',visible:visible(b),rect:b?rect(b):null,pills:[...(node?.querySelectorAll('[data-reaction-name]')||[])].map(e=>({{name:e.dataset.reactionName,visible:visible(e),rect:rect(e),tag:e.tagName,type:e.type,disabled:e.disabled,ariaDisabled:e.getAttribute('aria-disabled'),pressed:e.getAttribute('aria-pressed'),own:e.classList.contains('own'),count:Number(e.querySelector('.reaction-count')?.textContent),hit:(()=>{{const r=e.getBoundingClientRect(),p=document.elementFromPoint((r.left+r.right)/2,(r.top+r.bottom)/2);return p===e||e.contains(p);}})()}})),tools:node?.querySelectorAll('.message-tools').length||0}};v.ok=v.ok&&!!r&&r.text.includes({text})&&(r.reactions.find(p=>p.name==='wave')?.count||0)==={wave}&&v.editor?.sameNode==={same}&&v.editor?.editable==={editable}{notice};return v;}})()"
    );
    wait_dom(d, &expression).await?;
    checked(d,&format!("(()=>{{const r=[...document.querySelectorAll('[data-list-comment-ts]')].find(r=>r.dataset.listCommentTs==={ts});if(!r)return {{ok:false}};(r.querySelector('[data-reaction-name]')||r.querySelector('.message-body')||r).scrollIntoView({{block:'nearest'}});return {{ok:true}};}})()")).await?;
    let geometry = wait_dom(d, &expression).await?;
    let before = compose::snapshot(&state.peek(), model);
    let mut proof = proof.clone();
    proof["journal"] = compose::journal_snapshot(state).await?;
    let generation = before["comments"]["generation"]
        .as_u64()
        .ok_or("Comments generation missing")?;
    let pill_disabled = !crate::slack_lists::comments::reaction_actions::available(
        &state.peek(),
        generation,
        e.ts,
        "wave",
    );
    proof["reactionAvailable"] = json!(!pill_disabled);
    let body = &geometry["editBody"];
    if !ready(&before, &e)
        || before["noSession"] != true
        || before["noTransport"] != true
        || before["writes"] != 0
        || before["networkCalls"] != 0
        || geometry["editor"]["blocks"] != e.blocks
        || proof["journal"]["blocks"] != e.blocks
        || body["bold"] != e.bold
        || body["italic"] != e.italic
        || body["tools"] != 0
        || (e.text != "First page comment."
            && e.text != "Final review \u{4e2d}\u{1f642}"
            && body["visible"] != true)
        || body["pills"].as_array().is_none_or(|p| {
            p.iter().any(|p| {
                p["tag"] != "BUTTON"
                    || p["type"] != "button"
                    || p["visible"] != true
                    || p["hit"] != true
                    || p["disabled"] != pill_disabled
                    || p["ariaDisabled"] != pill_disabled.to_string()
                    || p["pressed"] != "false"
                    || p["own"] != false
                    || p["count"] != e.wave
            })
        })
    {
        return Err(format!(
            "Edit stage {name} lost exact draft/body/readonly geometry"
        ));
    }
    if !proof["originalMessage"].is_null()
        && before["composeLedger"]["messages"] != json!([proof["originalMessage"]])
    {
        return Err("Original POST response changed".into());
    }
    let capture = harness::capture(d, name).await?;
    if compose::snapshot(&state.peek(), model) != before
        || compose::journal_snapshot(state).await? != proof["journal"]
    {
        return Err(format!("Edit stage {name} changed during capture"));
    }
    rows.push(
        json!({"stage":name,"model":before,"geometry":geometry,"proof":proof,"capture":capture}),
    );
    Ok(())
}
pub(super) fn result(
    case: &str,
    readiness: Value,
    rows: Vec<Value>,
    events: Vec<Value>,
    state: Signal<ShellState>,
    model: &Model,
) -> Value {
    json!({"ok":true,"case":case,"fixture":readiness["fixture"],"readiness":readiness,"stages":rows,"editEvents":events,"final":compose::snapshot(&state.peek(),model),"initialRecords":fixture::initial_records(model),"noLiveNetwork":true})
}
fn expected(index: usize, proof: &Value) -> Expected<'static> {
    let reads = if index < 5 {
        4
    } else if index < 10 {
        8
    } else {
        12
    };
    let held = matches!(index, 5 | 6 | 7);
    let (text, bold, italic, wave) = match index {
        0 => ("First page comment.", true, false, 2),
        1 => ("Edited rich reply.", true, false, 2),
        2 => ("Edited plain reply.", false, false, 2),
        3..=6 => ("Cleared rich reply.", false, false, 0),
        7..=9 => ("Held edit reply.", false, false, 0),
        _ => ("Canonical readback reply.", false, true, 1),
    };
    Expected {
        reads,
        completed: reads - u64::from(matches!(index, 5 | 6)),
        cancelled: u64::from(index >= 7),
        held,
        loading: matches!(index, 5 | 6),
        posts: 0,
        busy: false,
        same: true,
        ts: FIRST,
        text,
        bold,
        italic,
        wave,
        notice: matches!(index, 7..=9).then_some(NOTICE),
        blocks: proof["originalA"].clone(),
    }
}
pub(super) async fn run(
    state: Signal<ShellState>,
    d: &dioxus::desktop::DesktopContext,
) -> Result<Value, String> {
    let (readiness, model, mut proof) = setup(state, d).await?;
    let mut rows = Vec::new();
    let mut events = Vec::new();
    capture(
        state,
        &model,
        d,
        STAGES[0],
        &proof,
        &mut rows,
        expected(0, &proof),
    )
    .await?;
    let mut rich = changed(FIRST, "Edited rich reply.");
    rich["message"]["blocks"] = blocks("Edited rich reply.", true);
    rich["message"]["edited"] = json!({"user":"U1","ts":"1781525100.000001"});
    rich["message"]["metadata"] =
        json!({"event_type":"list-edit","event_payload":{"keep":1,"old":true}});
    rich["message"]["opaque"] = json!({"ignored":true});
    let mut plain = changed(FIRST, "Edited plain reply.");
    plain["message"]["edited"] = json!({"ts":"1781525102.000001"});
    plain["message"]["metadata"] = json!({"event_type":"plain","event_payload":{"only":2}});
    let mut cleared = changed(FIRST, "Cleared rich reply.");
    cleared["message"]["blocks"] = json!([]);
    cleared["message"]["reactions"] = json!([]);
    cleared["message"]["edited"] = Value::Null;
    cleared["message"]["metadata"] = json!({"event_type":"cleared","event_payload":{"only":3}});
    for (index, frame, changed) in [
        (1, rich, true),
        (2, plain, true),
        (3, cleared.clone(), true),
        (4, cleared.clone(), false),
    ] {
        events.push(inject(
            state,
            &model,
            STAGES[index],
            "T1",
            7,
            frame,
            changed,
        )?);
        capture(
            state,
            &model,
            d,
            STAGES[index],
            &proof,
            &mut rows,
            expected(index, &proof),
        )
        .await?;
    }
    fixture::mode(&model, fixture::Mode::Held)?;
    click(d, "slack-list-comments-refresh").await?;
    wait_model(state, &model, |v| ready(v, &expected(5, &proof))).await?;
    proof["heldResponse"] = fixture::held_response(&state.peek(), &model)?;
    capture(
        state,
        &model,
        d,
        "held",
        &proof,
        &mut rows,
        expected(5, &proof),
    )
    .await?;
    events.push(inject(
        state,
        &model,
        "held-duplicate",
        "T1",
        7,
        cleared,
        false,
    )?);
    for (name, team, generation, outer, ts, root, nested, subtype) in [
        (
            "wrong-team",
            "T_OTHER",
            7,
            CHANNEL,
            FIRST,
            fixture::ROOT,
            None,
            "message_changed",
        ),
        (
            "stale-generation",
            "T1",
            6,
            CHANNEL,
            FIRST,
            fixture::ROOT,
            None,
            "message_changed",
        ),
        (
            "foreign-channel",
            "T1",
            7,
            "COTHER",
            FIRST,
            fixture::ROOT,
            None,
            "message_changed",
        ),
        (
            "conflicting-channel",
            "T1",
            7,
            CHANNEL,
            FIRST,
            fixture::ROOT,
            Some("COTHER"),
            "message_changed",
        ),
        (
            "wrong-root",
            "T1",
            7,
            CHANNEL,
            FIRST,
            "1781524998.000001",
            None,
            "message_changed",
        ),
        (
            "unknown-reply",
            "T1",
            7,
            CHANNEL,
            "1999999999.000001",
            fixture::ROOT,
            None,
            "message_changed",
        ),
        (
            "root",
            "T1",
            7,
            CHANNEL,
            fixture::ROOT,
            fixture::ROOT,
            None,
            "message_changed",
        ),
        (
            "replied",
            "T1",
            7,
            CHANNEL,
            FIRST,
            fixture::ROOT,
            None,
            "message_replied",
        ),
    ] {
        let mut frame = changed(ts, "Ignored edit.");
        frame["channel"] = json!(outer);
        frame["subtype"] = json!(subtype);
        frame["message"]["thread_ts"] = json!(root);
        if let Some(channel) = nested {
            frame["message"]["channel"] = json!(channel);
        }
        events.push(inject(state, &model, name, team, generation, frame, false)?);
    }
    capture(
        state,
        &model,
        d,
        "ignored-held",
        &proof,
        &mut rows,
        expected(6, &proof),
    )
    .await?;
    events.push(inject(
        state,
        &model,
        "updated-held",
        "T1",
        7,
        changed(FIRST, "Held edit reply."),
        true,
    )?);
    capture(
        state,
        &model,
        d,
        "updated-held",
        &proof,
        &mut rows,
        expected(7, &proof),
    )
    .await?;
    if fixture::held_response(&state.peek(), &model)? != proof["heldResponse"] {
        return Err("Held original response changed".into());
    }
    proof["release"] = fixture::release(&model)?;
    if proof["release"] != json!({"delivered":false,"cancelledBeforeReply":true}) {
        return Err("Original held read not cancelled".into());
    }
    capture(
        state,
        &model,
        d,
        "released",
        &proof,
        &mut rows,
        expected(8, &proof),
    )
    .await?;
    let mut unsupported = changed(FIRST, "Unsupported file body.");
    unsupported["message"]["files"] = json!([{"id":"FUNSUPPORTED1","mode":"hosted"}]);
    events.push(inject(
        state,
        &model,
        "unsupported",
        "T1",
        7,
        unsupported,
        false,
    )?);
    capture(
        state,
        &model,
        d,
        "unsupported",
        &proof,
        &mut rows,
        expected(9, &proof),
    )
    .await?;
    let raw = json!({"type":"message","channel":CHANNEL,"ts":FIRST,"thread_ts":fixture::ROOT,"user":"U1","text":"Canonical readback reply.","blocks":blocks("Canonical readback reply.",false),"reactions":[{"name":"wave","count":1,"users":["U1"]}],"edited":{"ts":"1781525109.000001"}});
    proof["serverResponse"] = fixture::publish_edit_readback(&state.peek(), &model, vec![raw])?;
    click(d, "slack-list-comments-refresh").await?;
    capture(
        state,
        &model,
        d,
        "readback",
        &proof,
        &mut rows,
        expected(10, &proof),
    )
    .await?;
    Ok(result("edits", readiness, rows, events, state, &model))
}
