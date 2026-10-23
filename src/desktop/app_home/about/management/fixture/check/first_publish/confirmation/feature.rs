//! Opt-in original checkbox interaction; bookmark and Feature outcomes stay separate.
use super::*;
use endpoint::confirmation::feature as backend;

pub(super) fn requested() -> bool {
    backend::requested()
}
fn case() -> String {
    std::env::var("SUPER_PLATINUM_WORKFLOW_FEATURE_CASE").unwrap_or_default()
}
fn feature(p: &Value) -> &Value {
    &add(p)["feature"]
}

const CHECKBOX: &str = r#"(()=>{
 const e=document.getElementById('workflow-confirmation-feature'),root=document.getElementById('workflow-management'),panel=document.getElementById('workflow-confirmation-add');
 const saved=root?.__workflowManagement?.permissionsConfirmation?.confirmationAdd;
 const registry=window.__firstPublishRegistry,id=n=>{if(!n)return null;if(!registry.ids.has(n))registry.ids.set(n,registry.next++);return registry.ids.get(n)};
 if(!e)return{ok:!EXPECTED,present:false,originalAbsent:!!saved&&saved.feature===null};
 e.scrollIntoView({block:'center',inline:'nearest',behavior:'instant'});
 const r=e.getBoundingClientRect(),x=r.left+r.width/2,y=r.top+r.height/2,h=document.elementFromPoint(x,y);
 const label=e.closest('label'),help=document.getElementById('workflow-confirmation-feature-help');
 const original=!!saved&&saved.feature===e&&saved.panel===panel&&saved.epoch===panel.dataset.addEpoch;
 const hit=h===e||e.contains(h),visible=r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1;
 return{ok:EXPECTED&&original&&e.isConnected&&e.type==='checkbox'&&!e.disabled&&!e.indeterminate&&visible&&hit,
 present:true,nodeId:id(e),original,connected:e.isConnected,type:e.type,checked:e.checked,disabled:e.disabled,indeterminate:e.indeterminate,
 label:label?.textContent.trim(),help:help?.textContent,epoch:panel.dataset.addEpoch,
 rect:{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height},visible,hit,hitPoint:{x,y}};
})()"#;
async fn checkbox(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
    visible: bool,
) -> Result<Value, String> {
    evidence::observed_wait(
        state,
        desktop,
        visit,
        "original Feature checkbox",
        &CHECKBOX.replace("EXPECTED", if visible { "true" } else { "false" }),
    )
    .await
}
// This action is evaluated once. No mutation is embedded in a polling predicate.
async fn activate(desktop: &DesktopContext, node: &Value) -> Result<Value, String> {
    let script=r#"(()=>{
 const e=document.getElementById('workflow-confirmation-feature'),root=document.getElementById('workflow-management'),saved=root?.__workflowManagement?.permissionsConfirmation?.confirmationAdd,registry=window.__firstPublishRegistry;
 if(!e||!e.isConnected||e.disabled||e.checked||e.indeterminate||saved?.feature!==e||registry?.ids.get(e)!==NODE)return{ok:false};
 e.scrollIntoView({block:'center',inline:'nearest',behavior:'instant'});e.focus();
 const r=e.getBoundingClientRect(),x=r.left+r.width/2,y=r.top+r.height/2,h=document.elementFromPoint(x,y);
 if(r.width<=0||r.height<=0||r.left<0||r.top<0||r.right>innerWidth+1||r.bottom>innerHeight+1||(h!==e&&!e.contains(h)))return{ok:false};
 let click=null;const listener=v=>{click={isTrusted:v.isTrusted,targetNodeId:registry.ids.get(v.target)}};
 e.addEventListener('click',listener,{once:true});e.click();e.removeEventListener('click',listener);
 return{ok:!!click&&e.checked,kind:'original-dom-click',nodeId:registry.ids.get(e),checked:e.checked,click,
 hitPoint:{x,y},rect:{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}};
})()"#.replace("NODE",&node["nodeId"].to_string());
    let (send, mut receive) = tokio::sync::mpsc::unbounded_channel();
    desktop
        .webview
        .evaluate_script_with_callback(&script, move |v| {
            let _ = send.send(v);
        })
        .map_err(|_| "Feature evaluation failed")?;
    let raw = tokio::time::timeout(Duration::from_secs(2), receive.recv())
        .await
        .map_err(|_| "Feature click timed out")?
        .ok_or("Feature click callback closed")?;
    let mut result: Value =
        serde_json::from_str(&raw).map_err(|_| "Invalid Feature click callback")?;
    if let Value::String(encoded) = result {
        result = serde_json::from_str(&encoded).map_err(|_| "Invalid encoded Feature click")?;
    }
    if result["ok"] != true {
        return Err(format!("Original Feature click refused: {result}"));
    }
    Ok(result)
}
pub(super) async fn selected(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
    mut selection: Value,
) -> Result<Value, String> {
    if !requested() {
        return Ok(selection);
    }
    until(
        state,
        visit,
        "optional Feature acquisition settled",
        |p, _| feature(p)["attempted"] == true && feature(p)["loading"] == false,
    )
    .await?;
    let allowed = case() != "denied";
    let checkbox_before = checkbox(state, desktop, visit, allowed).await?;
    let before = observe(state, desktop, visit).await?;
    if feature(&before["firstPublish"])["visible"] != allowed
        || feature(&before["firstPublish"])["checked"] != false
    {
        return Err("Original Feature availability differs from closed case".into());
    }
    let activation = if allowed {
        let value = activate(desktop, &checkbox_before).await?;
        until(state, visit, "actual Feature checkbox event", |p, _| {
            feature(p)["checked"] == true && feature(p)["loading"] == false
        })
        .await?;
        value
    } else {
        Value::Null
    };
    let checkbox_after = checkbox(state, desktop, visit, allowed).await?;
    let after = observe(state, desktop, visit).await?;
    if before["server"] != after["server"] {
        return Err("Checkbox dispatched backend work".into());
    }
    let original_choice_after = selection["after"].clone();
    selection["after"] = after.clone();
    selection["feature"] = json!({"case":case(),"choiceAfter":original_choice_after,"before":before,"activation":activation,"after":after,"checkboxBefore":checkbox_before,"checkboxAfter":checkbox_after});
    Ok(selection)
}
const PANEL: &str = r#"(()=>{
 const p=document.getElementById('channel-workflows-panel'),header=document.getElementById('channel-details-trigger'),notice=document.getElementById('channel-workflows-feature-status');
 if(!p||!p.isConnected||['idle','loading'].includes(p.dataset.status)||header?.textContent!=='#review-destination'||document.getElementById('workflow-finish-up'))return{ok:false};
 const expected=EXPECTED;let readback=null;try{readback=JSON.parse(p.dataset.fixtureReadback||'null')}catch{}
 if(p.dataset.status!=='ready'||readback?.current!==true||(readback.featured?.length??-1)!==expected.featured||(notice?.textContent??null)!==expected.notice)return{ok:false,status:p.dataset.status,readback,notice:notice?.textContent??null};
 const r=p.getBoundingClientRect();
 return{ok:true,connected:p.isConnected,status:p.dataset.status,readback,notice:notice?.textContent??null,
 rows:[...p.querySelectorAll('[data-trigger-id]')].map(e=>({id:e.dataset.triggerId,status:e.dataset.status,title:e.querySelector('.channel-workflow-title')?.textContent??null})),
 rect:{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height},modalAbsent:true};
})()"#;
async fn panel(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
    final_state: bool,
) -> Result<Value, String> {
    let scenario = case();
    let notice = if !final_state {
        Some("Featuring workflow\u{2026}")
    } else {
        match scenario.as_str() {
            "denied" => None,
            "rejected" => Some("Workflow added. Featuring could not be confirmed."),
            "readback-missing" => Some(
                "Workflow added. Featuring was acknowledged but is not yet visible in channel details.",
            ),
            "readback-failure" => Some(
                "Workflow added. Featuring was acknowledged, but channel details could not be verified.",
            ),
            _ => Some("Workflow featured in this channel."),
        }
    };
    let expected = json!({"notice":notice,"featured":if final_state&&matches!(scenario.as_str(),"success"|"held"){1}else{0}});
    evidence::observed_wait(
        state,
        desktop,
        visit,
        "Feature Workflows destination",
        &PANEL.replace("EXPECTED", &expected.to_string()),
    )
    .await
}
pub(super) async fn added(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
) -> Result<Value, String> {
    if !requested() {
        return Ok(Value::Null);
    }
    let scenario = case();
    let original = backend::capture(&state.peek())?;
    let mut held = Value::Null;
    if scenario == "held" {
        until(
            state,
            visit,
            "original Feature response held after Add",
            |p, s| {
                feature(p)["operation"]["outcome"] == "in-flight"
                    && s["confirmationAdd"]["feature"]["held"] == json!([1])
            },
        )
        .await?;
        let destination = panel(state, desktop, visit, false).await?;
        let before = observe(state, desktop, visit).await?;
        if add(&before["firstPublish"])["operation"]["navigated"] != true
            || last(&before["firstPublish"])["closed"] != true
        {
            return Err("Feature blocked accepted Add navigation".into());
        }
        let quiet = quiet(state, desktop, visit).await?;
        let release = original.release(1)?;
        if release["delivered"] != true || release["receiverRetired"] != false {
            return Err("Original held Feature receiver changed".into());
        }
        held = json!({"before":before,"destination":destination,"quiet":quiet,"release":release});
    }
    until(
        state,
        visit,
        "independent Feature terminal receipt",
        |p, _| {
            let op = &feature(p)["operation"];
            if scenario == "denied" {
                return op.is_null();
            }
            if scenario == "rejected" {
                return op["outcome"] == "rejected";
            }
            op["outcome"] == "acknowledged"
                && matches!(
                    op["readbackStatus"].as_str(),
                    Some("confirmed" | "absent" | "unavailable")
                )
        },
    )
    .await?;
    let destination = panel(state, desktop, visit, true).await?;
    let after = observe(state, desktop, visit).await?;
    Ok(json!({"case":scenario,"held":held,"destination":destination,"after":after}))
}
