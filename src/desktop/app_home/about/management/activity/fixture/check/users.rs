//! USER detail policy acquisition before any picker search.
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
async fn names(desktop: &DesktopContext) -> Result<Value, String> {
    let suffix = if case() == "user-real" {
        "profile real"
    } else {
        "display"
    };
    let expected = json!([
        format!("Ada {suffix}"),
        format!("Ada {suffix}"),
        format!("Zoe {suffix}")
    ]);
    let script=r#"(()=>{const expected=EXPECTED,els=[...document.querySelectorAll('#workflow-activity-detail .workflow-activity-user')];const rows=els.map(e=>{e.scrollIntoView({block:'nearest'});const r=e.getBoundingClientRect(),s=getComputedStyle(e),h=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);let visible=r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1&&s.visibility==='visible'&&(h===e||e.contains(h));for(let p=e.parentElement;p;p=p.parentElement){const b=p.getBoundingClientRect(),t=getComputedStyle(p);if(/auto|scroll|hidden|clip/.test(t.overflowY)&&(r.top<b.top-1||r.bottom>b.bottom+1))visible=false;}return{text:e.textContent,user:e.dataset.user,part:e.dataset.part,message:e.closest('.workflow-activity-message')?.id,tag:e.tagName,bold:Number(s.fontWeight)>=600,inert:!e.closest('button,a,[role=button]')&&!e.querySelector('button,a,input')&&!e.hasAttribute('tabindex')&&!e.hasAttribute('onclick'),visible,rect:{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}}});return{ok:rows.length===3&&rows.every((r,i)=>r.text===expected[i]&&r.user===(i<2?'UACTADA01':'UACTZOE01')&&r.part===(i===1?'3':'1')&&r.message===(i<2?'workflow-activity-trigger-message':'workflow-activity-step-message-0')&&r.bold&&r.inert&&r.visible),rows,measurement:'individual-scroll-samples'};})()"#.replace("EXPECTED",&expected.to_string());
    wait_dom(desktop, &script).await
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
    wait(state, "metadata editor", |v| {
        v["metadata"]["route"] == "editor" && v["metadata"]["busy"] == false
    })
    .await?;
    input(desktop, "workflow-management-title", TITLE).await?;
    input(desktop, "workflow-management-description", DESCRIPTION).await?;
    wait(state, "dirty draft", exact).await?;
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
    wait(state, "Activity list", |v| {
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
    endpoint::mode(&state.peek(), endpoint::Mode::Failure)?;
    click(desktop, "workflow-activity-run-E_REVIEW").await?;
    wait(state, "policy failure refuses detail publication", |v| {
        v["activity"]["detail"].is_null()
            && v["activity"]["notice"].is_string()
            && v["metadata"]["busy"] == false
    })
    .await?;
    let absent=wait_dom(desktop,"(()=>{const notice=document.getElementById('workflow-activity-notice');return{ok:document.querySelectorAll('.workflow-activity-user').length===0&&!!notice?.textContent,users:document.querySelectorAll('.workflow-activity-user').length,notice:notice?.textContent}})()").await?;
    stage(
        state,
        desktop,
        name,
        "policy-failed",
        absent,
        &mut stages,
        &mut captures,
    )
    .await?;
    click(desktop, "workflow-activity-close-detail").await?;
    click(desktop, "workflow-activity-run-E_REVIEW").await?;
    wait(state, "policy retry publishes detail", |v| {
        v["activity"]["detail"].is_object() && v["metadata"]["busy"] == false
    })
    .await?;
    let proof = names(desktop).await?;
    stage(
        state,
        desktop,
        name,
        "detail",
        proof,
        &mut stages,
        &mut captures,
    )
    .await?;
    click(desktop, "workflow-activity-back").await?;
    wait(state, "dirty draft return", |v| {
        exact(v) && v["activity"]["active"] == false
    })
    .await?;
    wait_dom(desktop,"({ok:!!document.getElementById('workflow-management-title')&&!document.getElementById('workflow-activity')})").await?;
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
    click(desktop, "workflow-management-activity").await?;
    wait(state, "Activity reentry", |v| {
        v["activity"]["active"] == true && v["metadata"]["busy"] == false
    })
    .await?;
    // Close retained detail through the actual control before requesting it again.
    let retained=wait_dom(desktop,"(()=>{const close=document.getElementById('workflow-activity-close-detail'),row=document.getElementById('workflow-activity-run-E_REVIEW');return{ok:!!(close||row),retained:!!close}})()").await?;
    if retained["retained"] == true {
        click(desktop, "workflow-activity-close-detail").await?;
    }
    // This case exercises cold explicit-detail cancellation. Automatic retained
    // detail invalidation has its own user-refresh scenario.
    let invalidation = endpoint::invalidate(&mut state.write())?;
    endpoint::mode(&state.peek(), endpoint::Mode::Held)?;
    click(desktop, "workflow-activity-run-E_REVIEW").await?;
    policy_wait(state, "held policy", |v| v["ledger"]["held"] == true).await?;
    let held=wait_dom(desktop,"({ok:document.querySelectorAll('.workflow-activity-user').length===0,users:document.querySelectorAll('.workflow-activity-user').length})").await?;
    stage(
        state,
        desktop,
        name,
        "held",
        json!({"invalidation":invalidation,"native":held}),
        &mut stages,
        &mut captures,
    )
    .await?;
    let mut replacement = replace_account(state)?;
    policy_wait(state, "retired original policy", |v| {
        v["ledger"]["cancelled"] == 1
    })
    .await?;
    replacement["lateReply"] = endpoint::release(&state.peek())?;
    if replacement["lateReply"]["cancelledBeforeReply"] != true {
        return Err("Retired policy reply was delivered".into());
    }
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
