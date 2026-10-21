use super::*;
pub(super) async fn wait_dom(desktop: &DesktopContext, script: &str) -> Result<Value, String> {
    let trace = include_str!("trace.js");
    let wrapped = format!(
        r#"(()=>{{{trace}
let measured;try{{measured=({script})}}catch(e){{measured={{ok:false,error:String(e)}}}}const root=document.getElementById('workflow-management');return{{ok:true,measured,diagnostic:{{calendar:workflowFixtureObservation('workflow-activity-date-range'),actionTrace:window.__workflowFixtureActionTrace?.rows??[],activity:!!document.getElementById('workflow-activity'),detail:!!document.getElementById('workflow-activity-detail'),dataset:root?{{...root.dataset}}:null,text:root?.textContent?.slice(0,4000),controls:root?[...root.querySelectorAll('button,input,textarea,select')].map(e=>({{id:e.id,value:e.value,disabled:e.disabled,readonly:e.readOnly}})):[]}}}}}})()"#
    );
    let mut last = Value::Null;
    for _ in 0..60 {
        last = crate::slack_links_check::wait_dom(desktop, &wrapped).await?;
        if last["measured"]["ok"] == true {
            return Ok(last["measured"].clone());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(format!("Activity DOM condition timed out: {last}"))
}
pub(super) async fn reveal_filters(desktop: &DesktopContext) -> Result<(), String> {
    wait_dom(desktop, r#"(()=>{const f=document.getElementById('workflow-activity-filter');if(!f)return{ok:false};workflowFixtureTrace('reveal-filter-before',f.id);if(f.getAttribute('aria-expanded')!=='true'){if(f.disabled)return{ok:false};f.scrollIntoView({block:'nearest'});const r=f.getBoundingClientRect(),h=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);if(h!==f&&!f.contains(h))return{ok:false};f.click();workflowFixtureTrace('reveal-filter-clicked',f.id);}return{ok:true}})()"#).await?;
    wait_dom(desktop, r#"(()=>{
const f=document.getElementById('workflow-activity-filter'),p=document.getElementById('workflow-activity-filter-panel'),d=document.getElementById('workflow-activity-date-range');
if(f?.getAttribute('aria-expanded')!=='true'||!p||!d||d.dataset.calendarPending!=='false')return{ok:false};
const s=getComputedStyle(p),r=p.getBoundingClientRect();
if(p.hidden||s.display==='none'||s.visibility!=='visible'||r.width<=0||r.height<=0||!p.getClientRects().length)return{ok:false};
// Calendar descendants can cover the Date Range trigger. The requested
// control's click/input helper owns its own reveal and interaction checks.
workflowFixtureTrace('reveal-filter-ready',f.id);return{ok:true};
})()"#).await?;
    Ok(())
}
pub(super) async fn reveal_control(desktop: &DesktopContext, id: &str) -> Result<(), String> {
    let needs = wait_dom(desktop, &format!(r#"(()=>{{const id={},e=document.getElementById(id);workflowFixtureTrace('reveal-control',id);return{{ok:!!e,filter:!!e?.closest('#workflow-activity-filter-panel'),outside:e?.id!=='workflow-activity-filter'&&!e?.closest('#workflow-activity-filter-panel'),filterOpen:document.getElementById('workflow-activity-filter')?.getAttribute('aria-expanded')==='true'}}}})()"#,json!(id))).await?;
    if needs["filter"] == true {
        reveal_filters(desktop).await?;
    } else if needs["outside"] == true && needs["filterOpen"] == true {
        // Deliberately close the real Filter before invoking an outside action;
        // do not race its pending calendar acknowledgment against that action.
        wait_dom(desktop,r#"(()=>{const f=document.getElementById('workflow-activity-filter');if(!f||f.disabled)return{ok:false};f.scrollIntoView({block:'nearest'});const r=f.getBoundingClientRect(),h=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);if(h!==f&&!f.contains(h))return{ok:false};workflowFixtureTrace('outside-filter-close-before',f.id);f.click();workflowFixtureTrace('outside-filter-close-clicked',f.id);return{ok:true}})()"#).await?;
        wait_dom(desktop,"({ok:document.getElementById('workflow-activity-filter')?.getAttribute('aria-expanded')==='false'&&document.getElementById('workflow-activity-date-range')?.dataset.calendarPending==='false'})").await?;
    }
    Ok(())
}
pub(super) async fn click(desktop: &DesktopContext, id: &str) -> Result<(), String> {
    reveal_control(desktop, id).await?;
    wait_dom(desktop,&format!(r#"(()=>{{const id={},e=document.getElementById(id);if(!e||e.disabled)return{{ok:false,observation:workflowFixtureTrace('click-unavailable',id)}};e.scrollIntoView({{block:'nearest'}});const r=e.getBoundingClientRect(),h=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);if(h!==e&&!e.contains(h))return{{ok:false,observation:workflowFixtureTrace('click-obscured',e.id)}};workflowFixtureTrace('click-before',e.id);e.click();workflowFixtureTrace('click-after',e.id);return{{ok:true}}}})()"#,json!(id))).await.map(|_|()).map_err(|e|format!("Click {id}: {e}"))
}
pub(super) async fn input(desktop: &DesktopContext, id: &str, value: &str) -> Result<(), String> {
    reveal_control(desktop, id).await?;
    wait_dom(desktop,&format!(r#"(()=>{{const e=document.getElementById({});if(!e||e.disabled||e.readOnly)return{{ok:false}};e.scrollIntoView({{block:'nearest'}});e.focus();const p=e.tagName==='SELECT'?HTMLSelectElement.prototype:e.tagName==='TEXTAREA'?HTMLTextAreaElement.prototype:HTMLInputElement.prototype;Object.getOwnPropertyDescriptor(p,'value').set.call(e,{});e.dispatchEvent(new Event('input',{{bubbles:true}}));e.dispatchEvent(new Event('change',{{bubbles:true}}));return{{ok:true}}}})()"#,json!(id),json!(value))).await.map(|_|()).map_err(|e|format!("Input {id}: {e}"))
}
pub(super) async fn capture(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    stage: &str,
    proof: Value,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    let model = snapshot(&state.peek());
    if model["metadata"]["networkCalls"] != 0 || model["ledger"]["networkCalls"] != 0 {
        return Err("Synthetic endpoint isolation changed".into());
    }
    let mut geometry = wait_dom(desktop, LAYOUT)
        .await
        .map_err(|e| format!("Layout {stage}: {e}"))?;
    let directory = std::env::var_os("SUPER_PLATINUM_UI_CAPTURE_DIR")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or("Absolute capture directory required")?;
    let path = directory.join(format!("{name}-{stage}.png"));
    let inspect = path.clone();
    if tokio::task::spawn_blocking(move || inspect.try_exists())
        .await
        .map_err(|_| "Inspect capture worker")?
        .map_err(|_| "Inspect capture path")?
    {
        return Err("Capture must be fresh".into());
    }
    eprintln!(
        "workflow-activity-capture: {}",
        json!({"stage":stage,"edge":"before"})
    );
    let (path, width, height) =
        crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), desktop).await?;
    geometry["pngDimensions"] = json!([width, height]);
    captures.insert(stage.into(), json!(path.to_string_lossy()));
    stages.push(json!({"stage":stage,"model":model,"geometry":geometry,"proof":proof}));
    eprintln!(
        "workflow-activity-capture: {}",
        json!({"stage":stage,"edge":"after"})
    );
    Ok(())
}
const LAYOUT: &str = r#"(()=>{
 const a=document.getElementById('workflow-activity'),m=document.getElementById('workflow-management'),config=document.getElementById('workflow-configuration'),source=!a&&!m&&!config?(document.getElementById('thread-body')||document.querySelector('.pins-panel')||document.getElementById('message-timeline')):null,root=a||m||source;
 const rect=e=>{const r=e.getBoundingClientRect();return{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}};
 // Only the actual viewport-fixed Filter/calendar escape ordinary overflow ancestors.
 // Retain the conservative full chain if an ancestor establishes its containing block.
 const fixedPopup=e=>{const c=e.closest('#workflow-activity-calendar,#workflow-activity-filter-panel');if(!c||getComputedStyle(c).position!=='fixed')return null;for(let p=c.parentElement;p;p=p.parentElement){const s=getComputedStyle(p);if(s.transform!=='none'||s.perspective!=='none'||s.filter!=='none'||(s.backdropFilter&&s.backdropFilter!=='none')||/(layout|paint|strict|content)/.test(s.contain)||/(transform|perspective|filter)/.test(s.willChange)||s.contentVisibility==='auto')return null;}return c;};
 const visible=e=>{const boundary=fixedPopup(e),r=rect(e),s=getComputedStyle(e);if(r.width<=0||r.height<=0||r.left<0||r.top<0||r.right>innerWidth+1||r.bottom>innerHeight+1||s.visibility!=='visible'||s.display==='none')return false;for(let p=e.parentElement;p;p=p.parentElement){const r2=rect(p),s2=getComputedStyle(p);if(/auto|scroll|hidden|clip/.test(s2.overflowY)&&(r.top<r2.top-1||r.bottom>r2.bottom+1))return false;if(/auto|scroll|hidden|clip/.test(s2.overflowX)&&(r.left<r2.left-1||r.right>r2.right+1))return false;if(p===boundary)break;}const h=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);return h===e||e.contains(h);};
 const filterOpen=document.getElementById('workflow-activity-filter')?.getAttribute('aria-expanded')==='true';
 const calendar=document.getElementById('workflow-activity-calendar');
 const displayed=e=>{for(let p=e;p;p=p.parentElement){const s=getComputedStyle(p);if(p.hidden||s.display==='none'||s.visibility==='hidden')return false;}return true;};
 const elements=a?[...a.querySelectorAll('button,input,select'),...document.querySelectorAll('#workflow-entry-back')]:m?[...m.querySelectorAll('.workflow-management-actions button,#workflow-entry-back,#workflow-entry-retry')]:config?[config]:source?[...source.querySelectorAll('.block-button[aria-label]'),...(source.id==='thread-body'?[...document.querySelectorAll('#thread-composer')]:source.id==='message-timeline'?[...document.querySelectorAll('#channel-composer')]:[])]:[];
 // Accessibility samples are taken after scrolling each control into view.
 // They are not a claim that a narrow scrolling surface shows all controls at once.
 const filterPanel=document.getElementById('workflow-activity-filter-panel');
 // An open Filter owns its controls; result rows/pagination remain behind it.
 // Navigation and all required feedback retain their ordinary strict checks.
 const filterElements=filterOpen&&filterPanel?[...filterPanel.querySelectorAll('button,input,select'),...document.querySelectorAll('#workflow-activity-filter,#workflow-activity-back,#workflow-entry-back')]:elements;
 const measuredElements=(calendar?[...calendar.querySelectorAll('button')]:filterElements).filter(displayed);
 const controls=measuredElements.map(e=>{e.scrollIntoView({block:'nearest'});return{id:e.id,disabled:e.disabled,visible:visible(e),rect:rect(e),value:e.value??null,text:e.textContent,scrollSample:true}});
 const detailStatusNode=document.getElementById('workflow-activity-detail-status');
 const sample=e=>{e.scrollIntoView({block:'nearest'});return{text:e.textContent,visible:visible(e),rect:rect(e),scrollSample:true}};
 const detailStatus=detailStatusNode?.textContent??null,detailStatusGeometry=detailStatusNode?sample(detailStatusNode):null;
 const lastStepStatuses=[...document.querySelectorAll('#workflow-activity-detail .workflow-activity-steps li:last-child > span:not(.muted)')].map(sample);
 const detailProof=!detailStatusNode||(detailStatus==='Processing'&&detailStatusGeometry.visible&&lastStepStatuses.length===1&&lastStepStatuses[0].text==='Completed'&&lastStepStatuses[0].visible);
 const target=calendar||(filterOpen?filterPanel:null)||a?.querySelector('#workflow-activity-notice')||a?.querySelector('#workflow-activity-detail')||a?.querySelector('#workflow-activity-empty')||a?.querySelector('.workflow-activity-results');
 if(target)target.scrollIntoView({block:'nearest'});
 const notices=root?[...root.querySelectorAll('[role="status"],[role="alert"],.workflow-management-feedback p,.workflow-activity-feedback p')].filter(e=>e.textContent.trim()&&displayed(e)&&(!calendar||calendar.contains(e))).map(e=>({text:e.textContent,visible:visible(e),rect:rect(e)})):[];
 const title=document.getElementById('workflow-management-title'),description=document.getElementById('workflow-management-description');
 const body=a?.querySelector('.workflow-activity-content')||a;
 const noOverflow=document.documentElement.scrollWidth<=innerWidth+1&&(!root||root.scrollWidth<=root.clientWidth+1);
 return{ok:noOverflow&&controls.length>0&&controls.every(c=>c.visible)&&notices.every(n=>n.visible)&&detailProof,viewport:[innerWidth,innerHeight],noOverflow,activity:!!a,filterOpen:a?filterOpen:false,calendarOpen:!!calendar,detail:!!document.getElementById('workflow-activity-detail'),detailStatus,detailStatusGeometry,lastStepStatuses,controls,notices,controlMeasurement:'individual-scroll-samples',captureControls:measuredElements.map(e=>({id:e.id,visible:visible(e),rect:rect(e)})),
 text:body?.textContent?.slice(0,8000)||null,fields:title&&description?{title:title.value,description:description.value,titleReadonly:title.readOnly,descriptionReadonly:description.readOnly}:null};
})()"#;
