//! Genuine member-search composition removes channel action authority.
use super::*;

fn state_composing(state: Signal<ShellState>) -> bool {
    let s = state.peek();
    s.app_home
        .about
        .management
        .surface
        .as_ref()
        .and_then(|h| crate::app_home::about::management::document(&s, h.id))
        .is_some_and(|d| d.activity.member_composing)
}

pub(super) async fn begin(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
) -> Result<Value, String> {
    let before = snapshot(&state.peek());
    let channel_before = endpoint::snapshot(&state.peek());
    let before_labels = channel_labels(desktop, false).await?;
    let special = case() == "channel-special";
    let script = r#"(()=>{
const input=document.getElementById('workflow-activity-user-search'),root=document.getElementById('workflow-management'),native=root?.__workflowManagement;
const ids=['workflow-activity-channel-trigger-1','workflow-activity-channel-trigger-3','workflow-activity-channel-step-0-1'];
if(SPECIAL)ids.push('workflow-activity-channel-step-1-1','workflow-activity-channel-step-1-3');
if(SHARED_FROZEN)ids.push('workflow-activity-channel-step-1-1','workflow-activity-channel-step-1-3','workflow-activity-channel-step-1-5','workflow-activity-channel-step-1-7');
const rows=ids.map((id,i)=>{const e=document.getElementById(id);return{originalButtonId:id,originalChannel:e?.dataset.channel,container:i<2?'workflow-activity-trigger-message':i===2?'workflow-activity-step-message-0':'workflow-activity-step-message-1',occurrence:i<2?i:i===2?0:i-3,text:e?.textContent}});
if(!input||!native||rows.some(r=>!r.originalChannel))return{ok:false};
window.__workflowChannelComposition={input,rows,refusals:native.channelRefusals||0};
input.dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true,data:'draft'}));
return{ok:true,composing:native.composing,value:input.value,refusals:native.channelRefusals||0};
})()"#.replace("SPECIAL", &json!(special).to_string())
        .replace("SHARED_FROZEN", &json!(endpoint::shared_frozen::requested()).to_string());
    let attempt = wait_dom(desktop, &script).await?;
    let mut inert = wait_dom(desktop, r#"(()=>{
const saved=window.__workflowChannelComposition,native=document.getElementById('workflow-management')?.__workflowManagement;
if(!saved||!native?.composing||document.getElementById('workflow-activity-user-search')!==saved.input)return{ok:false};
const rect=e=>{const r=e.getBoundingClientRect();return{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}};
const rows=saved.rows.map(original=>{
 const parent=document.getElementById(original.container),e=parent?.querySelectorAll('.workflow-activity-channel-label')[original.occurrence];
 if(!e)return{...original,ok:false};
 e.scrollIntoView({block:'nearest'});const r=rect(e),s=getComputedStyle(e),h=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);
 let visible=r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1&&s.display!=='none'&&s.visibility==='visible';
 for(let p=e.parentElement;p;p=p.parentElement){const q=rect(p),t=getComputedStyle(p);if(/auto|scroll|hidden|clip/.test(t.overflowY)&&(r.top<q.top-1||r.bottom>q.bottom+1))visible=false;if(/auto|scroll|hidden|clip/.test(t.overflowX)&&(r.left<q.left-1||r.right>q.right+1))visible=false;}
 const hit=h===e||e.contains(h),buttonPresent=!!document.getElementById(original.originalButtonId),interactiveAncestor=!!e.closest('button,a,[role=button]');
 return{...original,tag:e.tagName,text:e.textContent,buttonPresent,interactiveAncestor,visible,hit,rect:r,ok:e.tagName==='SPAN'&&e.textContent===original.text&&!buttonPresent&&!interactiveAncestor&&visible&&hit};
});
return{ok:rows.every(r=>r.ok),detailRevision:document.getElementById('workflow-activity-detail')?.dataset.detailRevision,rows};
})()"#).await?;
    let activation = wait_dom(desktop, r#"(()=>{
const original=window.__workflowChannelComposition?.rows[0],e=original&&document.getElementById(original.container)?.querySelectorAll('.workflow-activity-channel-label')[original.occurrence];
if(!e||e.tagName!=='SPAN'||e.textContent!==original.text||e.closest('button,a,[role=button]'))return{ok:false};
e.scrollIntoView({block:'nearest'});const r=e.getBoundingClientRect(),h=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);if(h!==e&&!e.contains(h))return{ok:false};
e.click();return{ok:true,event:'click',occurrence:0,observedMs:100};
})()"#).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;
    inert["activation"] = activation;
    if !state_composing(state) {
        return Err("Original member search is not composing".into());
    }
    let after = snapshot(&state.peek());
    let channel_after = endpoint::snapshot(&state.peek());
    if before != after {
        return Err("Inert composition changed original model snapshot".into());
    }
    if channel_before != channel_after {
        return Err("Inert composition dispatched or navigated".into());
    }
    let native = wait_dom(desktop, r#"(()=>{
const s=document.getElementById('workflow-management')?.__workflowManagement,saved=window.__workflowChannelComposition,n=document.getElementById('workflow-activity-navigation-notice');
return{ok:!!s?.composing&&(s.channelRefusals||0)===saved.refusals&&document.getElementById('workflow-activity-user-search')===saved.input,composing:s?.composing,refusals:s?.channelRefusals||0,notice:n?.textContent??'',value:saved?.input.value};
})()"#).await?;
    Ok(
        json!({"kind":"composition-refused","mode":"inert-while-composing","before":before,"after":after,"channelBefore":channel_before,"channelAfter":channel_after,"beforeLabels":before_labels,"stateComposing":true,"attempt":attempt,"native":native,"inert":inert,"observedMs":100}),
    )
}

pub(super) async fn finish(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
) -> Result<Value, String> {
    wait_dom(desktop, r#"(()=>{
const saved=window.__workflowChannelComposition;if(!saved||document.getElementById('workflow-activity-user-search')!==saved.input)return{ok:false};
saved.input.dispatchEvent(new CompositionEvent('compositionend',{bubbles:true,data:'draft'}));return{ok:true};
})()"#).await?;
    let labels = channel_labels(desktop, false).await?;
    let native = wait_dom(desktop, r#"(()=>{
const s=document.getElementById('workflow-management')?.__workflowManagement,saved=window.__workflowChannelComposition;
return{ok:!!s&&!s.composing&&(s.channelRefusals||0)===saved.refusals&&document.getElementById('workflow-activity-user-search')===saved.input,composing:s?.composing,value:saved?.input.value,refusals:s?.channelRefusals||0};
})()"#).await?;
    if state_composing(state) {
        return Err("Original member composition did not end".into());
    }
    Ok(
        json!({"stateComposing":false,"labels":labels,"native":native,"model":snapshot(&state.peek()),"channel":endpoint::snapshot(&state.peek())}),
    )
}
