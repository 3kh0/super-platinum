//! Retained USER detail refresh through actual preference events and closed reads.
use super::super::users as endpoint;
use super::*;
async fn stage(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    label: &str,
    exercise: Value,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    let policy = endpoint::snapshot(&state.peek());
    let proof = json!({"policy":policy,"exercise":exercise});
    capture(state, desktop, name, label, proof, stages, captures).await
}
async fn policy_wait(
    state: Signal<ShellState>,
    label: &str,
    test: impl Fn(&Value) -> bool,
) -> Result<Value, String> {
    for _ in 0..100 {
        let v = endpoint::snapshot(&state.peek());
        if test(&v) {
            return Ok(v);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!("{label}: {}", endpoint::snapshot(&state.peek())))
}
async fn names(desktop: &DesktopContext, real: bool) -> Result<Value, String> {
    let suffix = if real { "profile real" } else { "display" };
    let expected = json!([
        format!("Ada {suffix}"),
        format!("Ada {suffix}"),
        format!("Zoe {suffix}")
    ]);
    if case() == "user-deactivated" {
        return deactivated_names(desktop, expected).await;
    }
    let script=r#"(()=>{const expected=EXPECTED,els=[...document.querySelectorAll('#workflow-activity-detail .workflow-activity-user')];const rows=els.map(e=>{e.scrollIntoView({block:'nearest'});const r=e.getBoundingClientRect(),s=getComputedStyle(e),h=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);let visible=r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1&&s.visibility==='visible'&&(h===e||e.contains(h));for(let p=e.parentElement;p;p=p.parentElement){const b=p.getBoundingClientRect(),t=getComputedStyle(p);if(/auto|scroll|hidden|clip/.test(t.overflowY)&&(r.top<b.top-1||r.bottom>b.bottom+1))visible=false;}return{text:e.textContent,user:e.dataset.user,part:e.dataset.part,message:e.closest('.workflow-activity-message')?.id,tag:e.tagName,bold:Number(s.fontWeight)>=600,inert:!e.closest('button,a,[role=button]')&&!e.querySelector('button,a,input')&&!e.hasAttribute('tabindex')&&!e.hasAttribute('onclick'),visible,rect:{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}}});return{ok:rows.length===3&&rows.every((r,i)=>r.text===expected[i]&&r.user===(i<2?'UACTADA01':'UACTZOE01')&&r.part===(i===1?'3':'1')&&r.message===(i<2?'workflow-activity-trigger-message':'workflow-activity-step-message-0')&&r.bold&&r.inert&&r.visible),rows,measurement:'individual-scroll-samples'};})()"#.replace("EXPECTED",&expected.to_string());
    wait_dom(desktop, &script).await
}
async fn deactivated_names(desktop: &DesktopContext, expected: Value) -> Result<Value, String> {
    let script=r#"(()=>{
        const expected=EXPECTED,els=[...document.querySelectorAll('#workflow-activity-detail .workflow-activity-user')];
        const rect=e=>{const r=e.getBoundingClientRect();return{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}};
        const color=value=>{let s=value.trim();if(/^#[0-9a-f]{6}$/i.test(s))return[1,3,5].map(i=>parseInt(s.slice(i,i+2),16));if(/^#[0-9a-f]{3}$/i.test(s))return[1,2,3].map(i=>parseInt(s[i]+s[i],16));if(/^rgb/.test(s))return(s.match(/[\d.]+/g)||[]).slice(0,3).map(Number);return null;};
        const visible=e=>{if(!e)return false;const r=rect(e),s=getComputedStyle(e);if(r.width<=0||r.height<=0||r.left<0||r.top<0||r.right>innerWidth+1||r.bottom>innerHeight+1||s.visibility!=='visible'||s.display==='none')return false;for(let p=e.parentElement;p;p=p.parentElement){const b=rect(p),t=getComputedStyle(p);if(/auto|scroll|hidden|clip/.test(t.overflowY)&&(r.top<b.top-1||r.bottom>b.bottom+1))return false;}return [...e.getClientRects()].filter(r=>r.width>0&&r.height>0).every(r=>{const h=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);return h===e||e.contains(h)});};
        const fragments=e=>e?[...e.getClientRects()].filter(r=>r.width>0&&r.height>0).map(r=>{const h=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);return{hit:h===e||e.contains(h),rect:{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}}}):[];
        const ordinaryColor=els[2]?color(getComputedStyle(els[2]).color):null;
        const rows=els.map((e,i)=>{
            e.scrollIntoView({block:'nearest'});
            const label=e.querySelector(':scope > .workflow-activity-user-name'),marker=e.querySelector(':scope > .workflow-activity-user-deactivated'),s=getComputedStyle(e),ms=marker?getComputedStyle(marker):null;
            const deleted=i<2,parentColor=color(s.color),mutedColor=color(s.getPropertyValue('--muted'));
            const content=[...e.childNodes].filter(n=>n.nodeType!==Node.COMMENT_NODE&&!(n.nodeType===Node.TEXT_NODE&&n.textContent===''));
            const shape=content.length===(deleted?2:1)&&content.every(n=>n.nodeType===Node.ELEMENT_NODE)&&e.children[0]===label&&(!deleted||e.children[1]===marker);
            return{text:e.textContent,rawName:label?.textContent,nameTag:label?.tagName,nameCount:e.querySelectorAll('.workflow-activity-user-name').length,nameVisible:visible(label),nameRect:label?rect(label):null,
                indicator:marker?.textContent??null,indicatorTag:marker?.tagName??null,indicatorCount:e.querySelectorAll('.workflow-activity-user-deactivated').length,indicatorVisible:marker?visible(marker):null,indicatorRect:marker?rect(marker):null,
                indicatorMargin:ms?parseFloat(ms.marginInlineStart):null,indicatorWeight:ms?Number(ms.fontWeight):null,indicatorColor:ms?color(ms.color):null,
                fragments:fragments(e),nameFragments:fragments(label),indicatorFragments:fragments(marker),
                deactivated:e.dataset.deactivated,deactivatedClass:e.classList.contains('workflow-activity-user--deactivated'),shape,parentColor,mutedColor,ordinaryColor,opacity:Number(s.opacity),
                user:e.dataset.user,part:e.dataset.part,message:e.closest('.workflow-activity-message')?.id,tag:e.tagName,bold:Number(s.fontWeight)>=600,
                inert:!e.closest('button,a,[role=button]')&&!e.querySelector('button,a,input,[tabindex],[onclick]')&&!e.hasAttribute('tabindex')&&!e.hasAttribute('onclick'),visible:visible(e),rect:rect(e)};
        });
        return{ok:rows.length===3&&rows.every((r,i)=>{const deleted=i<2;return r.rawName===expected[i]&&r.text===expected[i]+(deleted?'(deactivated)':'')&&r.nameTag==='SPAN'&&r.nameCount===1&&r.nameVisible&&r.shape&&r.deactivated===String(deleted)&&r.deactivatedClass===deleted&&r.opacity===1&&r.user===(deleted?'UACTADA01':'UACTZOE01')&&r.part===(i===1?'3':'1')&&r.message===(deleted?'workflow-activity-trigger-message':'workflow-activity-step-message-0')&&r.bold&&r.inert&&r.visible&&(deleted?r.indicator==='(deactivated)'&&r.indicatorTag==='SPAN'&&r.indicatorCount===1&&r.indicatorVisible&&r.indicatorMargin===4&&r.indicatorWeight===400&&r.mutedColor!==null&&JSON.stringify(r.parentColor)===JSON.stringify(r.mutedColor)&&JSON.stringify(r.indicatorColor)===JSON.stringify(r.mutedColor)&&JSON.stringify(r.parentColor)!==JSON.stringify(r.ordinaryColor):r.indicator===null&&r.indicatorCount===0)}),rows,measurement:'individual-scroll-samples'};
    })()"#.replace("EXPECTED",&expected.to_string());
    wait_dom(desktop, &script).await
}
const FAILURE: &str =
    "Names could not be refreshed. Close the run details and reopen them to try again.";
fn prefs(v: &Value) -> usize {
    v["ledger"]["requests"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["method"] == "users.prefs.get")
        .count()
}
fn event(mut state: Signal<ShellState>) -> Result<Value, String> {
    let before = state.peek().app_home.workflow_names_revision;
    let proof = endpoint::invalidate(&mut state.write())?;
    super::super::super::refresh::after_event(state, before);
    Ok(proof)
}
async fn settled(state: Signal<ShellState>, count: usize) -> Result<(), String> {
    policy_wait(state, "policy refresh request", |v| {
        prefs(v) == count
            && v["ledger"]["completed"].as_u64()
                == v["ledger"]["requests"].as_array().map(|r| r.len() as u64)
    })
    .await?;
    wait(state, "refresh settled", |v| {
        v["activity"]["nameRefreshPending"] == false && v["metadata"]["busy"] == false
    })
    .await?;
    Ok(())
}
async fn footer(desktop: &DesktopContext) -> Result<Value, String> {
    let script=r#"(()=>{const n=document.getElementById('workflow-activity-name-notice');if(!n)return{ok:false};const r=n.getBoundingClientRect(),s=getComputedStyle(n),h=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2),visible=r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1&&s.visibility==='visible'&&(h===n||n.contains(h)),outsideScroll=!n.closest('.workflow-activity-content'),globalToastCount=document.querySelectorAll('#status-toast').length;return{ok:n.textContent===EXPECTED&&visible&&outsideScroll&&globalToastCount===0,notice:n.textContent,visible,outsideScroll,globalToastCount,rect:{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}}})()"#.replace("EXPECTED",&json!(FAILURE).to_string());
    wait_dom(desktop, &script).await
}
async fn back(state: Signal<ShellState>, desktop: &DesktopContext) -> Result<(), String> {
    click(desktop, "workflow-activity-back").await?;
    wait(state, "retained dirty editor", |v| {
        exact(v) && v["activity"]["active"] == false
    })
    .await?;
    wait_dom(desktop,"({ok:!!document.getElementById('workflow-management-title')&&!document.getElementById('workflow-activity')})").await?;
    Ok(())
}
async fn reopen(state: Signal<ShellState>, desktop: &DesktopContext) -> Result<(), String> {
    click(desktop, "workflow-management-activity").await?;
    wait(state, "retained Activity", |v| {
        v["activity"]["active"] == true && v["activity"]["detail"].is_object()
    })
    .await?;
    wait_dom(
        desktop,
        "({ok:!!document.getElementById('workflow-activity-detail')})",
    )
    .await?;
    Ok(())
}
async fn retry(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    count: usize,
) -> Result<Value, String> {
    click(desktop, "workflow-activity-close-detail").await?;
    click(desktop, "workflow-activity-run-E_REVIEW").await?;
    settled(state, count).await?;
    names(desktop, true).await
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    let readiness = readiness(name).await?;
    let mut stages = vec![];
    let mut captures = serde_json::Map::new();
    stage(
        state,
        desktop,
        name,
        "about",
        Value::Null,
        &mut stages,
        &mut captures,
    )
    .await?;
    click(desktop, "workflow-configuration").await?;
    wait(state, "editor", |v| {
        v["metadata"]["route"] == "editor" && v["metadata"]["busy"] == false
    })
    .await?;
    input(desktop, "workflow-management-title", TITLE).await?;
    input(desktop, "workflow-management-description", DESCRIPTION).await?;
    wait(state, "dirty", exact).await?;
    stage(
        state,
        desktop,
        name,
        "draft",
        Value::Null,
        &mut stages,
        &mut captures,
    )
    .await?;
    click(desktop, "workflow-management-activity").await?;
    wait(state, "list", |v| {
        v["activity"]["page"].is_object() && v["metadata"]["busy"] == false
    })
    .await?;
    stage(
        state,
        desktop,
        name,
        "list",
        Value::Null,
        &mut stages,
        &mut captures,
    )
    .await?;
    click(desktop, "workflow-activity-run-E_REVIEW").await?;
    settled(state, 1).await?;
    let original = snapshot(&state.peek())["activity"]["detail"].clone();
    let proof = names(desktop, true).await?;
    stage(
        state,
        desktop,
        name,
        "detail-real",
        proof,
        &mut stages,
        &mut captures,
    )
    .await?;
    endpoint::policy_override(&state.peek(), -1)?;
    let inv = event(state)?;
    settled(state, 2).await?;
    let proof = names(desktop, false).await?;
    stage(
        state,
        desktop,
        name,
        "event-display",
        json!({"invalidation":inv,"names":proof}),
        &mut stages,
        &mut captures,
    )
    .await?;
    back(state, desktop).await?;
    stage(
        state,
        desktop,
        name,
        "warm-return",
        Value::Null,
        &mut stages,
        &mut captures,
    )
    .await?;
    reopen(state, desktop).await?;
    let proof = names(desktop, false).await?;
    stage(
        state,
        desktop,
        name,
        "warm-reentry",
        proof,
        &mut stages,
        &mut captures,
    )
    .await?;
    back(state, desktop).await?;
    endpoint::policy_override(&state.peek(), 1)?;
    let inv = event(state)?;
    tokio::time::sleep(Duration::from_millis(350)).await;
    stage(
        state,
        desktop,
        name,
        "editor-invalidated",
        json!({"invalidation":inv}),
        &mut stages,
        &mut captures,
    )
    .await?;
    reopen(state, desktop).await?;
    settled(state, 3).await?;
    let proof = names(desktop, true).await?;
    stage(
        state,
        desktop,
        name,
        "reentry-real",
        proof,
        &mut stages,
        &mut captures,
    )
    .await?;
    for (label, mode, count, retry_label) in [
        ("offline", endpoint::Mode::Failure, 4, "offline-retried"),
        (
            "malformed",
            endpoint::Mode::Malformed,
            6,
            "malformed-retried",
        ),
    ] {
        endpoint::mode(&state.peek(), mode)?;
        let inv = event(state)?;
        settled(state, count).await?;
        let proof = footer(desktop).await?;
        if snapshot(&state.peek())["activity"]["detail"] != original {
            return Err("Refresh replaced raw detail".into());
        }
        stage(
            state,
            desktop,
            name,
            label,
            json!({"invalidation":inv,"footer":proof}),
            &mut stages,
            &mut captures,
        )
        .await?;
        let proof = retry(state, desktop, count + 1).await?;
        stage(
            state,
            desktop,
            name,
            retry_label,
            proof,
            &mut stages,
            &mut captures,
        )
        .await?;
    }
    for label in ["raw-refused", "composition-refused"] {
        let before = snapshot(&state.peek());
        let requests = endpoint::snapshot(&state.peek())["ledger"]["requests"].clone();
        if label == "raw-refused" {
            wait_dom(desktop,"(()=>{const e=document.getElementById('workflow-activity-from');e.value='2026-06-01';return{ok:e.value==='2026-06-01'}})()").await?;
        } else {
            wait_dom(desktop,"(()=>{const e=document.getElementById('workflow-activity-from');e.dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true,data:'2'}));return{ok:document.getElementById('workflow-management').__workflowManagement?.composing===true}})()").await?;
        }
        let inv = event(state)?;
        tokio::time::sleep(Duration::from_millis(500)).await;
        let after = snapshot(&state.peek());
        if requests != endpoint::snapshot(&state.peek())["ledger"]["requests"]
            || before["activity"]["detail"] != after["activity"]["detail"]
            || before["activity"]["filters"] != after["activity"]["filters"]
            || after["activity"]["nameNotice"] != Value::Null
            || after["metadata"]["busy"] != false
        {
            return Err("Refused refresh changed buffers, policy requests or detail".into());
        }
        let native=wait_dom(desktop,"(()=>{const e=document.getElementById('workflow-activity-from');return{ok:!!e,value:e?.value,composing:!!document.getElementById('workflow-management').__workflowManagement?.composing}})()").await?;
        stage(state,desktop,name,label,json!({"invalidation":inv,"before":before,"after":after,"native":native,"requestsUnchanged":true}),&mut stages,&mut captures).await?;
        if label == "raw-refused" {
            wait_dom(desktop,"(()=>{document.getElementById('workflow-activity-from').value='';return{ok:true}})()").await?;
        } else {
            wait_dom(desktop,"(()=>{document.getElementById('workflow-activity-from').dispatchEvent(new CompositionEvent('compositionend',{bubbles:true,data:'2'}));return{ok:!document.getElementById('workflow-management').__workflowManagement?.composing}})()").await?;
        }
    }
    endpoint::mode(&state.peek(), endpoint::Mode::Held)?;
    let inv = event(state)?;
    policy_wait(state, "held refresh", |v| {
        prefs(v) == 8 && v["ledger"]["held"] == true
    })
    .await?;
    stage(
        state,
        desktop,
        name,
        "held",
        json!({"invalidation":inv}),
        &mut stages,
        &mut captures,
    )
    .await?;
    endpoint::policy_override(&state.peek(), -1)?;
    let inv = event(state)?;
    policy_wait(state, "newer revision replaces held refresh", |v| {
        prefs(v) == 9 && v["ledger"]["cancelled"] == 1
    })
    .await?;
    let release = endpoint::release(&state.peek())?;
    settled(state, 9).await?;
    let proof = names(desktop, false).await?;
    stage(
        state,
        desktop,
        name,
        "latest-display",
        json!({"invalidation":inv,"release":release,"names":proof}),
        &mut stages,
        &mut captures,
    )
    .await?;
    back(state, desktop).await?;
    stage(
        state,
        desktop,
        name,
        "returned",
        Value::Null,
        &mut stages,
        &mut captures,
    )
    .await?;
    reopen(state, desktop).await?;
    names(desktop, false).await?;
    endpoint::policy_override(&state.peek(), 1)?;
    endpoint::mode(&state.peek(), endpoint::Mode::Held)?;
    let inv = event(state)?;
    policy_wait(state, "account held refresh", |v| {
        prefs(v) == 10 && v["ledger"]["held"] == true
    })
    .await?;
    stage(
        state,
        desktop,
        name,
        "account-held",
        json!({"invalidation":inv}),
        &mut stages,
        &mut captures,
    )
    .await?;
    let mut replacement = replace_account(state)?;
    policy_wait(state, "account retired refresh", |v| {
        v["ledger"]["cancelled"] == 2
    })
    .await?;
    replacement["lateReply"] = endpoint::release(&state.peek())?;
    wait_dom(desktop,"({ok:!!document.getElementById('workflow-configuration')&&!document.getElementById('workflow-activity')&&!document.getElementById('workflow-management')})").await?;
    stage(
        state,
        desktop,
        name,
        "account-replaced",
        replacement.clone(),
        &mut stages,
        &mut captures,
    )
    .await?;
    Ok(
        json!({"ok":true,"fixture":name,"case":case(),"readiness":readiness,"viewport":stages.last().unwrap()["geometry"]["viewport"],"aboutConfiguration":true,"actualActivityEntry":true,"noLiveNetwork":true,"stages":stages,"captures":captures,"replacement":replacement,"final":snapshot(&state.peek())}),
    )
}
