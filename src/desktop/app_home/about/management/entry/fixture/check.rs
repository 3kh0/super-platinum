//! Real conversation action -> owned workflow entry, with original composer custody.
use super::*;
use crate::app_home::about::management::entry::fixture as entry_fixture;
use crate::app_home::about::management::fixture as metadata_fixture;

async fn action(d: &DesktopContext, label: &str) -> Result<(), String> {
    wait_dom(d,&format!(r#"(()=>{{const e=[...document.querySelectorAll('.block-button')].find(e=>e.getAttribute('aria-label')==={});if(!e||e.disabled)return{{ok:false}};e.scrollIntoView({{block:'nearest'}});const r=e.getBoundingClientRect(),h=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);if(h!==e&&!e.contains(h))return{{ok:false}};e.click();return{{ok:true}}}})()"#,json!(label))).await?;
    Ok(())
}
async fn composer(d: &DesktopContext, seed: bool) -> Result<Value, String> {
    wait_dom(d,&format!(r#"(()=>{{const seed={},value={},e=document.getElementById('channel-composer');if(!e)return{{ok:false}};if(seed&&!window.__executionComposerFixture){{if(!e.dataset.projectionVisit||!e.dataset.completionReady)return{{ok:false}};e.focus();Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,'value').set.call(e,value);e.setSelectionRange(3,10,'forward');window.__executionComposerFixture={{node:e,visit:e.dataset.projectionVisit,scope:e.dataset.draftScope,value,start:3,end:10}};}}const saved=window.__executionComposerFixture,r=e.getBoundingClientRect(),style=getComputedStyle(e),visible=r.width>0&&r.height>0&&style.display!=='none'&&style.visibility==='visible';return{{ok:!!saved&&saved.node===e&&saved.visit===e.dataset.projectionVisit&&saved.scope===e.dataset.draftScope&&e.value===saved.value&&e.selectionStart===saved.start&&e.selectionEnd===saved.end,sameNode:saved?.node===e,visit:e.dataset.projectionVisit,scope:e.dataset.draftScope,value:e.value,start:e.selectionStart,end:e.selectionEnd,visible}}}})()"#,seed,json!(entry_fixture::COMPOSER))).await
}
async fn header(d: &DesktopContext, seed: bool) -> Result<Value, String> {
    wait_dom(d,&format!(r#"(()=>{{const seed={seed},name=document.getElementById('channel-details-trigger'),header=name?.closest('.conversation-header'),conversation=header?.closest('.conversation');if(!header||!conversation)return{{ok:false}};if(seed&&!window.__executionHeaderFixture)window.__executionHeaderFixture={{header,name,conversation,scrollTop:conversation.scrollTop}};const saved=window.__executionHeaderFixture,r=header.getBoundingClientRect(),style=getComputedStyle(header),hit=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2),visible=r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth&&r.bottom<=innerHeight&&style.visibility==='visible'&&style.display==='flex'&&(hit===header||header.contains(hit));const sameHeader=header===saved?.header,sameName=name===saved?.name,sameConversation=conversation===saved?.conversation,text=name.textContent;return{{ok:sameHeader&&sameName&&sameConversation&&text==='#ship'&&visible&&conversation.scrollTop===0&&conversation.scrollTop===saved.scrollTop,sameHeader,sameName,sameConversation,text,display:style.display,visible,scrollTop:conversation.scrollTop,originalScrollTop:saved?.scrollTop,rect:{{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}}}}}})()"#)).await
}
async fn stage(
    s: Signal<ShellState>,
    d: &DesktopContext,
    name: &str,
    label: &str,
    exercise: Value,
    rows: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    let model = snapshot(&s.peek());
    let expected = json!({"entry":model["entry"]["active"],"activity":model["activity"]["active"]==true,
        "detail":model["activity"]["detail"].is_object(),
        "notice":if model["activity"]["active"]==true{model["activity"]["notice"].clone()}else{model["entry"]["notice"].clone()}});
    wait_dom(d,&format!(r#"(()=>{{const expected={expected},m=document.getElementById('workflow-management'),a=document.getElementById('workflow-activity'),n=document.getElementById(expected.activity?'workflow-activity-notice':'workflow-entry-notice');return{{ok:!!m===expected.entry&&!!a===expected.activity&&!!document.getElementById('workflow-activity-detail')===expected.detail&&(!m||!!m.__workflowManagement&&!m.__workflowManagement.lease&&!m.inert)&&(!expected.notice||n?.textContent===expected.notice),expected,notice:n?.textContent??null,lease:!!m?.__workflowManagement?.lease}}}})()"#)).await?;
    let source = entry_fixture::snapshot(&s.peek());
    let buffer = if label == "account-replaced" {
        Value::Null
    } else {
        composer(d, false).await?
    };
    let conversation_header = if matches!(label, "transcript" | "returned" | "route-retired") {
        header(d, label == "transcript").await?
    } else {
        Value::Null
    };
    capture(
        s,
        d,
        name,
        label,
        json!({"source":source,"composer":buffer,"header":conversation_header,"exercise":exercise}),
        rows,
        captures,
    )
    .await
}
pub(super) async fn opened(s: Signal<ShellState>, d: &DesktopContext) -> Result<(), String> {
    wait(s, "Direct matching execution opened", |v| {
        v["entry"]["active"] == true
            && v["entry"]["busy"] == false
            && v["entry"]["autoPending"] == false
            && v["activity"]["detail"]["workflow_event"]["workflow_execution_id"] == FIRST
    })
    .await?;
    wait_dom(d,"({ok:!!document.getElementById('workflow-activity-detail')&&document.getElementById('workflow-activity-detail-status')?.textContent==='Processing'})").await?;
    Ok(())
}
async fn conversation(s: Signal<ShellState>, d: &DesktopContext) -> Result<(), String> {
    click(d, "workflow-entry-back").await?;
    wait(s, "Direct route closed", |v| v["entry"]["active"] == false).await?;
    wait_dom(d,&format!(r#"(()=>{{const e=[...document.querySelectorAll('.block-button')].find(e=>e.getAttribute('aria-label')==={}),r=e?.getBoundingClientRect();return{{ok:!!r&&r.width>0&&r.height>0&&!document.getElementById('workflow-management')}}}})()"#,json!(entry_fixture::OPEN))).await?;
    composer(d, false).await?;
    Ok(())
}
async fn refused_back(s: Signal<ShellState>, d: &DesktopContext) -> Result<Value, String> {
    const RAW: &str = "Uncommitted workflow title";
    const NOTICE: &str =
        "Finish editing the current text before returning. Your draft is unchanged.";
    let before = snapshot(&s.peek());
    let source_before = entry_fixture::snapshot(&s.peek());
    wait_dom(d,&format!(r#"(()=>{{const root=document.getElementById('workflow-management'),title=document.getElementById('workflow-management-title'),description=document.getElementById('workflow-management-description'),notice=document.getElementById('workflow-entry-navigation-notice');if(!root||!title||!notice||title.value!=={}||description?.value!=={}||root.inert||root.__workflowManagement?.lease)return{{ok:false}};window.__executionBackFixture={{root,title,notice,original:title.value}};Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(title,{});return{{ok:true}}}})()"#,json!(TITLE),json!(DESCRIPTION),json!(RAW))).await?;
    click(d, "workflow-entry-back").await?;
    let native=wait_dom(d,&format!(r#"(()=>{{const saved=window.__executionBackFixture,root=document.getElementById('workflow-management'),title=document.getElementById('workflow-management-title'),notice=document.getElementById('workflow-entry-navigation-notice');if(!saved||!notice)return{{ok:false}};notice.scrollIntoView({{block:'nearest'}});const r=notice.getBoundingClientRect(),style=getComputedStyle(notice),hit=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2),visible=r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth&&r.bottom<=innerHeight&&style.visibility==='visible'&&style.display!=='none'&&(hit===notice||notice.contains(hit));const sameRoot=root===saved.root,sameTitle=title===saved.title,sameNotice=notice===saved.notice;return{{ok:sameRoot&&sameTitle&&sameNotice&&title.value==={}&&notice.textContent==={}&&visible&&!root.inert&&!root.__workflowManagement?.lease,sameRoot,sameTitle,sameNotice,value:title?.value,notice:notice.textContent,visible,rect:{{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}}}}}})()"#,json!(RAW),json!(NOTICE))).await?;
    let after = snapshot(&s.peek());
    let source_after = entry_fixture::snapshot(&s.peek());
    if before != after || source_before != source_after {
        return Err("Refused Back changed model or retained document".into());
    }
    let restored=wait_dom(d,r#"(()=>{const s=window.__executionBackFixture,e=document.getElementById('workflow-management-title');if(!s||e!==s.title||document.getElementById('workflow-management')!==s.root)return{ok:false};Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,s.original);return{ok:e.value===s.original,sameTitle:e===s.title,value:e.value}})()"#).await?;
    Ok(
        json!({"backRefusal":{"before":before,"after":after,"sourceBefore":source_before,"sourceAfter":source_after,"native":native,"restored":restored}}),
    )
}
fn count(s: Signal<ShellState>) -> usize {
    snapshot(&s.peek())["ledger"]["requests"]
        .as_array()
        .unwrap()
        .len()
}
async fn read(s: Signal<ShellState>, before: usize) -> Result<(), String> {
    wait(s, "Activity read settled", |v| {
        v["metadata"]["busy"] == false
            && v["ledger"]["requests"]
                .as_array()
                .is_some_and(|r| r.len() == before + 1)
    })
    .await?;
    Ok(())
}
pub(super) async fn run(
    mut state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    let readiness = readiness(name).await?;
    let scenario = case();
    let mut stages = vec![];
    let mut captures = serde_json::Map::new();
    let original = composer(desktop, true).await?;
    let before = snapshot(&state.peek());
    let malformed=wait_dom(desktop,&format!(r#"(()=>{{const e=[...document.querySelectorAll('.block-button')].find(e=>e.getAttribute('aria-label')==={});if(!e?.disabled)return{{ok:false}};e.click();return{{ok:true,disabled:e.disabled,label:e.getAttribute('aria-label')}}}})()"#,json!(entry_fixture::MALFORMED))).await?;
    if snapshot(&state.peek()) != before {
        return Err("Malformed client action changed entry or dispatched".into());
    }
    stage(
        state,
        desktop,
        name,
        "transcript",
        json!({"initialComposer":original,"malformed":malformed,"unchangedAfterMalformed":true}),
        &mut stages,
        &mut captures,
    )
    .await?;
    if scenario == "execution-failure" {
        metadata_fixture::set_mode(&mut state.write(), metadata_fixture::Mode::PolicyDenied);
        action(desktop, entry_fixture::OPEN).await?;
        wait(state, "Direct policy refused", |v| {
            v["entry"]["active"] == true
                && v["entry"]["busy"] == false
                && v["entry"]["notice"].is_string()
        })
        .await?;
        stage(
            state,
            desktop,
            name,
            "policy-denied",
            Value::Null,
            &mut stages,
            &mut captures,
        )
        .await?;
        metadata_fixture::set_mode(&mut state.write(), metadata_fixture::Mode::Success);
        click(desktop, "workflow-entry-retry").await?;
        opened(state, desktop).await?;
        stage(
            state,
            desktop,
            name,
            "policy-retried",
            Value::Null,
            &mut stages,
            &mut captures,
        )
        .await?;
        click(desktop, "workflow-activity-close-detail").await?;
        wait_dom(
            desktop,
            "({ok:!document.getElementById('workflow-activity-detail')})",
        )
        .await?;
        mode(&state.peek(), Mode::Malformed)?;
        let before = count(state);
        click(desktop, "workflow-activity-apply").await?;
        read(state, before).await?;
        wait(state, "Activity failure retained", |v| {
            v["activity"]["notice"].is_string() && v["activity"]["detail"].is_null()
        })
        .await?;
        stage(
            state,
            desktop,
            name,
            "read-failed",
            Value::Null,
            &mut stages,
            &mut captures,
        )
        .await?;
        let before = count(state);
        click(desktop, "workflow-activity-apply").await?;
        read(state, before).await?;
        stage(
            state,
            desktop,
            name,
            "read-retried",
            Value::Null,
            &mut stages,
            &mut captures,
        )
        .await?;
        conversation(state, desktop).await?;
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
    } else if scenario == "execution-retirement" {
        mode(&state.peek(), Mode::Held)?;
        action(desktop, entry_fixture::OPEN).await?;
        wait(state, "Held direct Activity read", |v| {
            v["ledger"]["held"].is_u64() && v["entry"]["active"] == true
        })
        .await?;
        stage(
            state,
            desktop,
            name,
            "route-held",
            Value::Null,
            &mut stages,
            &mut captures,
        )
        .await?;
        conversation(state, desktop).await?;
        wait(state, "Route retirement cancelled read", |v| {
            v["ledger"]["cancelled"] == 1
        })
        .await?;
        let released = release(&state.peek())?;
        stage(
            state,
            desktop,
            name,
            "route-retired",
            released,
            &mut stages,
            &mut captures,
        )
        .await?;
        mode(&state.peek(), Mode::Held)?;
        action(desktop, entry_fixture::OPEN).await?;
        wait(state, "Second held direct Activity read", |v| {
            v["ledger"]["held"].is_u64()
                && v["ledger"]["requests"]
                    .as_array()
                    .is_some_and(|r| r.len() == 2)
        })
        .await?;
        stage(
            state,
            desktop,
            name,
            "account-held",
            Value::Null,
            &mut stages,
            &mut captures,
        )
        .await?;
        let mut replacement = replace_account(state)?;
        wait(state, "Account retirement cancelled read", |v| {
            v["ledger"]["cancelled"] == 2
        })
        .await?;
        replacement["release"] = release(&state.peek())?;
        wait_dom(desktop,"({ok:!document.getElementById('workflow-management')&&!!document.getElementById('workflow-configuration')})").await?;
        stage(
            state,
            desktop,
            name,
            "account-replaced",
            replacement,
            &mut stages,
            &mut captures,
        )
        .await?;
    } else {
        action(desktop, entry_fixture::OPEN).await?;
        opened(state, desktop).await?;
        stage(
            state,
            desktop,
            name,
            "opened",
            Value::Null,
            &mut stages,
            &mut captures,
        )
        .await?;
        click(desktop, "workflow-activity-close-detail").await?;
        wait_dom(
            desktop,
            "({ok:!document.getElementById('workflow-activity-detail')})",
        )
        .await?;
        stage(
            state,
            desktop,
            name,
            "closed",
            Value::Null,
            &mut stages,
            &mut captures,
        )
        .await?;
        input(desktop, "workflow-activity-status", "SUCCESS").await?;
        input(desktop, "workflow-activity-limit", "20").await?;
        let before = count(state);
        click(desktop, "workflow-activity-apply").await?;
        read(state, before).await?;
        stage(
            state,
            desktop,
            name,
            "filtered",
            Value::Null,
            &mut stages,
            &mut captures,
        )
        .await?;
        let before = count(state);
        click(desktop, "workflow-activity-next").await?;
        read(state, before).await?;
        stage(
            state,
            desktop,
            name,
            "paged",
            Value::Null,
            &mut stages,
            &mut captures,
        )
        .await?;
        click(desktop, "workflow-activity-back").await?;
        wait_dom(desktop,"({ok:!!document.getElementById('workflow-management-title')&&!document.getElementById('workflow-activity')})").await?;
        input(desktop, "workflow-management-title", TITLE).await?;
        input(desktop, "workflow-management-description", DESCRIPTION).await?;
        wait(state, "Exact dirty metadata", exact).await?;
        let refusal = refused_back(state, desktop).await?;
        stage(
            state,
            desktop,
            name,
            "metadata-draft",
            refusal,
            &mut stages,
            &mut captures,
        )
        .await?;
        conversation(state, desktop).await?;
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
        action(desktop, entry_fixture::OPEN).await?;
        opened(state, desktop).await?;
        wait(state, "Exact dirty metadata reopened", exact).await?;
        stage(
            state,
            desktop,
            name,
            "reopened",
            Value::Null,
            &mut stages,
            &mut captures,
        )
        .await?;
        conversation(state, desktop).await?;
        let before = count(state);
        action(desktop, entry_fixture::MISSING).await?;
        wait(
            state,
            "Missing execution consumed without lookup or pagination",
            |v| {
                v["entry"]["active"] == true
                    && v["entry"]["busy"] == false
                    && v["entry"]["autoPending"] == false
                    && v["activity"]["page"].is_object()
                    && v["activity"]["detail"].is_null()
                    && v["ledger"]["requests"]
                        .as_array()
                        .is_some_and(|r| r.len() == before + 1)
            },
        )
        .await?;
        stage(
            state,
            desktop,
            name,
            "missing-target",
            json!({"readCountBefore":before,"readCountAfter":count(state)}),
            &mut stages,
            &mut captures,
        )
        .await?;
    }
    Ok(
        json!({"ok":true,"fixture":name,"case":scenario,"readiness":readiness,"viewport":stages.last().unwrap()["geometry"]["viewport"],"actualMessageAction":true,"aboutConfiguration":false,"actualActivityEntry":true,"noLiveNetwork":true,"stages":stages,"captures":captures,"final":snapshot(&state.peek())}),
    )
}
