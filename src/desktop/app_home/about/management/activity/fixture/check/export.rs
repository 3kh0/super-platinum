//! Closed export acceptance through real native controls and claimed request plans.
use super::super::super::export::fixture as endpoint;
use super::*;
const REFUSAL: &str =
    "Finish editing the current filters before exporting. Your draft is unchanged.";

fn ledger(s: Signal<ShellState>) -> Value {
    endpoint::snapshot(&s.peek())
}
fn exports(s: Signal<ShellState>) -> Value {
    snapshot(&s.peek())["export"].clone()
}
async fn phase(s: Signal<ShellState>, expected: &str) -> Result<Value, String> {
    wait(s, &format!("Export phase {expected}"), |v| {
        v["export"]["attempts"]
            .as_array()
            .and_then(|a| a.last())
            .is_some_and(|a| a["phase"] == expected)
            && v["metadata"]["busy"] == (expected == "sent")
    })
    .await
}
async fn enter(s: Signal<ShellState>, d: &DesktopContext) -> Result<(), String> {
    click(d, "workflow-management-activity").await?;
    wait(s, "Activity entry", |v| {
        v["activity"]["active"] == true
            && v["metadata"]["busy"] == false
            && v["activity"]["page"].is_object()
    })
    .await?;
    wait_dom(
        d,
        "({ok:!!document.getElementById('workflow-activity-filters')})",
    )
    .await?;
    Ok(())
}
async fn back(s: Signal<ShellState>, d: &DesktopContext) -> Result<(), String> {
    click(d, "workflow-activity-back").await?;
    wait(s, "Exact draft returned", |v| {
        exact(v) && v["activity"]["active"] == false
    })
    .await?;
    wait_dom(d,"({ok:!!document.getElementById('workflow-management-title')&&!document.getElementById('workflow-activity')})").await?;
    Ok(())
}
async fn policy(
    s: Signal<ShellState>,
    d: &DesktopContext,
    value: endpoint::Policy,
) -> Result<(), String> {
    back(s, d).await?;
    endpoint::policy(&s.peek(), value)?;
    let before = snapshot(&s.peek())["metadata"]["reads"].as_u64().unwrap();
    click(d, "workflow-management-check").await?;
    wait(s, "Actual policy reacquisition", |v| {
        exact(v)
            && v["metadata"]["busy"] == false
            && v["metadata"]["reads"].as_u64().is_some_and(|n| n > before)
    })
    .await?;
    enter(s, d).await
}
async fn dom(d: &DesktopContext) -> Result<Value, String> {
    wait_dom(d,r#"(()=>{const e=document.getElementById('workflow-activity-export'),n=document.getElementById('workflow-activity-export-notice'),r=document.getElementById('workflow-activity-export-native-notice'),u=document.getElementById('workflow-activity-user');const box=e=>{const r=e.getBoundingClientRect();return{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}};const measure=e=>{if(!e||!e.textContent.trim())return null;const b=box(e),s=getComputedStyle(e),h=document.elementFromPoint(b.left+b.width/2,b.top+b.height/2);let visible=b.width>0&&b.height>0&&b.left>=0&&b.top>=0&&b.right<=innerWidth+1&&b.bottom<=innerHeight+1&&s.visibility==='visible'&&s.display!=='none'&&(h===e||e.contains(h));for(let p=e.parentElement;p;p=p.parentElement){const q=box(p),t=getComputedStyle(p);if(/auto|scroll|hidden|clip/.test(t.overflowY)&&(b.top<q.top-1||b.bottom>q.bottom+1))visible=false;}return{text:e.textContent,visible,rect:b}};return{ok:true,button:e?{disabled:e.disabled,text:e.textContent}:null,notice:measure(n),nativeNotice:measure(r),user:u?.value??null,userLabel:u?.selectedOptions?.[0]?.textContent??null,from:document.getElementById('workflow-activity-from')?.value??null,to:document.getElementById('workflow-activity-to')?.value??null,status:document.getElementById('workflow-activity-status')?.value??null,limit:document.getElementById('workflow-activity-limit')?.value??null,refusals:document.getElementById('workflow-management')?.__workflowManagement?.exportRefusals??0}})()"#).await
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
    // A model phase can advance before its controlled render and the original
    // native lease's next-frame release. Do not measure that transient frame.
    let expected = {
        let shell = s.peek();
        let model = snapshot(&shell);
        let editor = model["metadata"]["route"] == "editor";
        let active = model["activity"]["active"] == true;
        let surface = shell.app_home.about.management.surface.as_ref();
        json!({"editor":editor,"active":active,
            "visit":surface.map(|h|h.id.to_string()),
            "document":surface.and_then(|h|h.document).map(|id|id.to_string()),
            "revision":model["activity"]["documentRevision"].as_u64().map(|id|id.to_string()),
            "notice":if active{model["export"]["current"]["notice"].clone()}else{Value::Null},
            "shown":active && model["export"]["current"]["shown"]==true,
            "disabled":model["export"]["current"]["disabled"]})
    };
    let rendered = wait_dom(d, &format!(r#"(()=>{{
        const expected={expected},root=document.getElementById('workflow-management'),
          activity=document.getElementById('workflow-activity'),form=document.getElementById('workflow-activity-filters'),
          notice=document.getElementById('workflow-activity-export-notice'),button=document.getElementById('workflow-activity-export');
        const text=notice?.textContent?.trim()?notice.textContent:null;
        const state=root?.__workflowManagement;
        const owner=expected.editor?!!root?.isConnected&&!!state&&root.dataset.managementVisit===expected.visit
          &&root.dataset.managementDocument===expected.document&&root.dataset.managementRevision===expected.revision:!root;
        const released=!root||!!state&&!state.lease&&!root.inert;
        const route=expected.active?!!activity?.isConnected&&!!form?.isConnected&&root?.contains(activity)
          &&activity.contains(form)&&!activity.inert&&!form.inert:!activity&&!form;
        const action=expected.shown?!!button&&button.disabled===expected.disabled:!button;
        return{{ok:owner&&released&&route&&action&&text===expected.notice,
          expected,owner,released,route,action,notice:text,leased:!!state?.lease}};
    }})()"#)).await.map_err(|error|format!("Export render {label}: {error}"))?;
    let native = dom(d).await?;
    for key in ["notice", "nativeNotice"] {
        if native[key].is_object() && native[key]["visible"] != true {
            return Err(format!("Export feedback clipped at {label}: {native}"));
        }
    }
    capture(
        s,
        d,
        name,
        label,
        json!({"export":exports(s),"ledger":ledger(s),"native":native,"exercise":exercise,"rendered":rendered}),
        rows,
        captures,
    )
    .await
}
async fn refusal(s: Signal<ShellState>, d: &DesktopContext, ime: bool) -> Result<Value, String> {
    let before = snapshot(&s.peek());
    let probe=wait_dom(d,&format!(r#"(()=>{{const e=document.getElementById('workflow-activity-from'),r=document.getElementById('workflow-management');if(!e||!r?.__workflowManagement)return{{ok:false}};{}return{{ok:true,value:e.value,refusals:r.__workflowManagement.exportRefusals??0}}}})()"#,
        if ime {"e.dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true,data:'x'}));"}else{"Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,'2026-06-01');"})).await?;
    click(d, "workflow-activity-export").await?;
    let value = if ime {
        before["activity"]["filters"]["from"].as_str().unwrap_or("")
    } else {
        "2026-06-01"
    };
    let after_dom=wait_dom(d,&format!(r#"(()=>{{const e=document.getElementById('workflow-activity-from'),r=document.getElementById('workflow-management'),n=document.getElementById('workflow-activity-export-native-notice');return{{ok:e?.value==={}&&n?.textContent==={}&&(r?.__workflowManagement?.exportRefusals??0)==={},value:e?.value,notice:n?.textContent,refusals:r?.__workflowManagement?.exportRefusals}}}})()"#,json!(value),json!(REFUSAL),probe["refusals"].as_u64().unwrap()+1)).await?;
    let after = snapshot(&s.peek());
    if before != after {
        return Err(format!(
            "Native export refusal wrote Rust state: before={before},after={after}"
        ));
    }
    Ok(
        json!({"unchangedRustState":true,"before":probe,"after":after_dom,"composition":ime,"rawValue":value}),
    )
}
async fn reset_refusal(d: &DesktopContext, ime: bool) -> Result<(), String> {
    wait_dom(d,if ime {"(()=>{document.getElementById('workflow-activity-from').dispatchEvent(new CompositionEvent('compositionend',{bubbles:true,data:'x'}));return{ok:true}})()"} else {"(()=>{Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(document.getElementById('workflow-activity-from'),'2026-06-15');return{ok:true}})()"}).await?;
    Ok(())
}
async fn send(
    s: Signal<ShellState>,
    d: &DesktopContext,
    mode: endpoint::Mode,
    expected: &str,
) -> Result<(), String> {
    let count = ledger(s)["requests"].as_array().unwrap().len();
    endpoint::mode(&s.peek(), mode)?;
    click(d, "workflow-activity-export").await?;
    wait(s, "Export request entered once", |v| {
        v["exportFixture"]["requests"]
            .as_array()
            .is_some_and(|a| a.len() == count + 1)
    })
    .await?;
    phase(s, expected).await?;
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
    if scenario == "export" {
        endpoint::policy(&state.peek(), endpoint::Policy::Missing)?;
    }
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
    wait(state, "Metadata editor", |v| {
        v["metadata"]["route"] == "editor" && v["metadata"]["busy"] == false
    })
    .await?;
    input(desktop, "workflow-management-title", TITLE).await?;
    input(desktop, "workflow-management-description", DESCRIPTION).await?;
    wait(state, "Retained draft", exact).await?;
    let selected = endpoint::initial_user(&mut state.write())?;
    stage(
        state,
        desktop,
        name,
        "draft",
        selected.clone(),
        &mut stages,
        &mut captures,
    )
    .await?;
    enter(state, desktop).await?;
    if scenario == "export" {
        wait_dom(
            desktop,
            "({ok:!document.getElementById('workflow-activity-export')})",
        )
        .await?;
        stage(
            state,
            desktop,
            name,
            "gate-missing",
            Value::Null,
            &mut stages,
            &mut captures,
        )
        .await?;
        policy(state, desktop, endpoint::Policy::Off).await?;
        wait_dom(
            desktop,
            "({ok:!document.getElementById('workflow-activity-export')})",
        )
        .await?;
        stage(
            state,
            desktop,
            name,
            "gate-off",
            Value::Null,
            &mut stages,
            &mut captures,
        )
        .await?;
        policy(state, desktop, endpoint::Policy::On).await?;
    }
    input(desktop, "workflow-activity-status", "SUCCESS").await?;
    input(desktop, "workflow-activity-from", "2026-06-15").await?;
    input(desktop, "workflow-activity-to", "2026-06-16").await?;
    input(desktop, "workflow-activity-limit", "20").await?;
    click(desktop, "workflow-activity-apply").await?;
    wait(state, "Filtered page", |v| {
        v["metadata"]["busy"] == false && v["activity"]["query"]["limit"] == 20
    })
    .await?;
    let dates=wait_dom(desktop,"(()=>{const a=new Date(2026,5,15),b=new Date(2026,5,16,23,59,59);return{ok:true,from:Math.floor(a.getTime()/1000),to:Math.floor(b.getTime()/1000)}})()").await?;
    stage(
        state,
        desktop,
        name,
        if scenario == "export" {
            "ready"
        } else {
            "list"
        },
        json!({"dates":dates,"selected":selected}),
        &mut stages,
        &mut captures,
    )
    .await?;
    if scenario == "export" {
        let proof = refusal(state, desktop, false).await?;
        stage(
            state,
            desktop,
            name,
            "raw-refused",
            proof,
            &mut stages,
            &mut captures,
        )
        .await?;
        reset_refusal(desktop, false).await?;
        let proof = refusal(state, desktop, true).await?;
        stage(
            state,
            desktop,
            name,
            "composition-refused",
            proof,
            &mut stages,
            &mut captures,
        )
        .await?;
        reset_refusal(desktop, true).await?;
        endpoint::policy(&state.peek(), endpoint::Policy::Off)?;
        click(desktop, "workflow-activity-export").await?;
        phase(state, "not-sent").await?;
        if !ledger(state)["requests"].as_array().unwrap().is_empty() {
            return Err("Changed preflight sent export".into());
        }
        stage(
            state,
            desktop,
            name,
            "preflight-refused",
            json!({"noSend":true}),
            &mut stages,
            &mut captures,
        )
        .await?;
        policy(state, desktop, endpoint::Policy::On).await?;
        click(desktop, "workflow-activity-children-E_REVIEW").await?;
        wait(state, "Nested export source", |v| {
            v["metadata"]["busy"] == false
                && v["activity"]["query"]["parent_workflow_execution_id"] == "E_REVIEW"
        })
        .await?;
        send(state, desktop, endpoint::Mode::Held, "sent").await?;
        let before = ledger(state);
        let duplicate=wait_dom(desktop,"(()=>{const e=document.getElementById('workflow-activity-export');if(!e?.disabled)return{ok:false};e.click();e.click();return{ok:true,disabled:e.disabled}})()").await?;
        if ledger(state) != before {
            return Err("Duplicate disabled click replayed request".into());
        }
        stage(
            state,
            desktop,
            name,
            "held",
            duplicate,
            &mut stages,
            &mut captures,
        )
        .await?;
        let released = endpoint::release(&state.peek(), endpoint::Mode::Success)?;
        phase(state, "initiated").await?;
        stage(
            state,
            desktop,
            name,
            "initiated",
            released,
            &mut stages,
            &mut captures,
        )
        .await?;
        click(desktop, "workflow-activity-all").await?;
        wait(state, "All-run export source", |v| {
            v["metadata"]["busy"] == false
                && v["activity"]["query"]
                    .get("parent_workflow_execution_id")
                    .is_none()
        })
        .await?;
        click(desktop, "workflow-activity-next").await?;
        wait(state, "Cursor export source", |v| {
            v["metadata"]["busy"] == false && v["activity"]["query"]["cursor"].is_string()
        })
        .await?;
        for (label, mode, expected) in [
            ("rejected", endpoint::Mode::Rejected, "rejected"),
            ("retried", endpoint::Mode::Success, "initiated"),
            ("offline", endpoint::Mode::Offline, "not-sent"),
            ("offline-retried", endpoint::Mode::Success, "initiated"),
            ("malformed", endpoint::Mode::Malformed, "unknown"),
        ] {
            send(state, desktop, mode, expected).await?;
            stage(
                state,
                desktop,
                name,
                label,
                Value::Null,
                &mut stages,
                &mut captures,
            )
            .await?;
        }
        let before = ledger(state);
        input(desktop, "workflow-activity-status", "ERROR").await?;
        input(desktop, "workflow-activity-limit", "50").await?;
        click(desktop, "workflow-activity-apply").await?;
        wait(state, "Equivalent uppercase filters loaded", |v| {
            v["metadata"]["busy"] == false && v["activity"]["query"]["limit"] == 50
        })
        .await?;
        click(desktop, "workflow-activity-next").await?;
        wait(state, "Equivalent page loaded", |v| {
            v["metadata"]["busy"] == false && v["activity"]["query"]["cursor"].is_string()
        })
        .await?;
        let blocked=wait_dom(desktop,"(()=>{const e=document.getElementById('workflow-activity-export');if(!e?.disabled)return{ok:false};e.click();return{ok:true,disabled:e.disabled}})()").await?;
        if ledger(state) != before {
            return Err("Unknown effective query replayed".into());
        }
        stage(
            state,
            desktop,
            name,
            "unknown-blocked",
            blocked,
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
    } else {
        send(state, desktop, endpoint::Mode::Held, "sent").await?;
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
        back(state, desktop).await?;
        let release = endpoint::release(&state.peek(), endpoint::Mode::Lost)?;
        phase(state, "unknown").await?;
        stage(
            state,
            desktop,
            name,
            "route-retired",
            release,
            &mut stages,
            &mut captures,
        )
        .await?;
        enter(state, desktop).await?;
        input(desktop, "workflow-activity-from", "2026-06-14").await?;
        send(state, desktop, endpoint::Mode::Held, "sent").await?;
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
        let mut replaced = endpoint::replace_account(state)?;
        replaced["release"] = endpoint::release(&state.peek(), endpoint::Mode::Success)?;
        phase(state, "initiated").await?;
        wait_dom(desktop,"({ok:!!document.getElementById('workflow-configuration')&&!document.getElementById('workflow-management')})").await?;
        stage(
            state,
            desktop,
            name,
            "account-replaced",
            replaced,
            &mut stages,
            &mut captures,
        )
        .await?;
    }
    Ok(
        json!({"ok":true,"fixture":name,"case":scenario,"readiness":readiness,"viewport":stages.last().unwrap()["geometry"]["viewport"],"aboutConfiguration":true,"actualActivityEntry":true,"noLiveNetwork":true,"stages":stages,"captures":captures,"final":snapshot(&state.peek()),"scope":"Initial selected user is fixture setup; requests are synthetic endpoint replies, never delivery/file completion."}),
    )
}
