use super::*;
const REFUSAL: &str =
    "Finish editing the current text before opening this List. Your draft is unchanged.";
pub(super) async fn buffer(
    s: Signal<ShellState>,
    d: &DesktopContext,
    kind: &str,
) -> Result<Value, String> {
    native::reveal_filters(d).await?;
    native::reveal_control(d, FIRST_LINK).await?;
    let before = snapshot(&s.peek());
    let files = endpoint::snapshot(&s.peek());
    let (id, value) = if kind == "filter-refused" {
        ("workflow-activity-status", "ERROR")
    } else {
        ("workflow-activity-user-search", "Uncommitted native search")
    };
    let attempt=wait_dom(d,&format!(r#"(()=>{{
 const e=document.getElementById({}),n=document.getElementById('workflow-management')?.__workflowManagement;
 if(!e||!n)return{{ok:false}};
 Object.getOwnPropertyDescriptor(e.tagName==='SELECT'?HTMLSelectElement.prototype:HTMLInputElement.prototype,'value').set.call(e,{});
 return{{ok:true,value:e.value,refusals:n.channelRefusals||0}};
 }})()"#,json!(id),json!(value))).await?;
    click(d, FIRST_LINK).await?;
    let native=wait_dom(d,&format!(r#"(()=>{{const n=document.getElementById('workflow-management')?.__workflowManagement,e=document.getElementById({}),notice=document.getElementById('workflow-activity-navigation-notice');return{{ok:n?.channelRefusals==={}&&e?.value==={}&&notice?.textContent==={},value:e?.value,refusals:n?.channelRefusals,notice:notice?.textContent}}}})()"#,json!(id),attempt["refusals"].as_u64().unwrap()+1,json!(value),json!(REFUSAL))).await?;
    if snapshot(&s.peek()) != before || endpoint::snapshot(&s.peek()) != files {
        return Err("Refusal changed original model or file requests".into());
    }
    Ok(
        json!({"kind":kind,"attempt":attempt,"native":native,"before":before,"filesBefore":files,"unchangedRustState":true}),
    )
}
pub(super) async fn restore(d: &DesktopContext, kind: &str) -> Result<(), String> {
    let id = if kind == "filter-refused" {
        "workflow-activity-status"
    } else {
        "workflow-activity-user-search"
    };
    wait_dom(d,&format!(r#"(()=>{{const e=document.getElementById({});if(!e)return{{ok:false}};Object.getOwnPropertyDescriptor(e.tagName==='SELECT'?HTMLSelectElement.prototype:HTMLInputElement.prototype,'value').set.call(e,'');document.getElementById('workflow-activity-navigation-notice').textContent='';return{{ok:true}}}})()"#,json!(id))).await?;
    Ok(())
}
pub(super) async fn composition(
    s: Signal<ShellState>,
    d: &DesktopContext,
) -> Result<Value, String> {
    native::reveal_filters(d).await?;
    native::reveal_control(d, FIRST_LINK).await?;
    let before = snapshot(&s.peek());
    let files = endpoint::snapshot(&s.peek());
    let original = labels(d, false).await?;
    let attempt=wait_dom(d,r#"(()=>{
 const input=document.getElementById('workflow-activity-user-search'),n=document.getElementById('workflow-management')?.__workflowManagement;
 if(!input||!n)return{ok:false};window.__activityFileComposition={input,refusals:n.channelRefusals||0};
 input.dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true,data:'draft'}));
 return{ok:true,value:input.value,refusals:n.channelRefusals||0};
 })()"#).await?;
    let inert = labels(d, true).await?;
    let activation=wait_dom(d,r#"(()=>{
 const e=document.querySelector('#workflow-activity-trigger-message .workflow-activity-file');
 if(!e||e.tagName!=='SPAN'||e.closest('a,button'))return{ok:false};e.click();return{ok:true,tag:e.tagName,text:e.textContent};
 })()"#).await?;
    tokio::time::sleep(Duration::from_millis(150)).await;
    if snapshot(&s.peek()) != before || endpoint::snapshot(&s.peek()) != files {
        return Err("Inert composition changed original state or dispatched".into());
    }
    Ok(
        json!({"before":before,"filesBefore":files,"originalLabels":original,"attempt":attempt,
        "inert":inert,"activation":activation,"observedMs":150,"unchangedRustState":true}),
    )
}
pub(super) async fn composition_end(d: &DesktopContext) -> Result<(), String> {
    wait_dom(d,r#"(()=>{const saved=window.__activityFileComposition,input=document.getElementById('workflow-activity-user-search');if(!saved||saved.input!==input)return{ok:false};input.dispatchEvent(new CompositionEvent('compositionend',{bubbles:true,data:'draft'}));return{ok:true}})()"#).await?;
    labels(d, false).await?;
    wait_dom(d,r#"(()=>{const n=document.getElementById('workflow-management')?.__workflowManagement,s=window.__activityFileComposition;return{ok:!!n&&!n.composing&&(n.channelRefusals||0)===s.refusals&&document.getElementById('workflow-activity-user-search')===s.input}})()"#).await?;
    Ok(())
}
pub(super) async fn calendar(s: Signal<ShellState>, d: &DesktopContext) -> Result<Value, String> {
    let before = snapshot(&s.peek());
    let files = endpoint::snapshot(&s.peek());
    let destination_before = destination::snapshot(&s.peek());
    let original_labels = labels(d, false).await?;
    native::reveal_filters(d).await?;
    click(d, "workflow-activity-date-range").await?;
    wait_dom(d,"({ok:!!document.getElementById('workflow-activity-calendar')&&document.getElementById('workflow-activity-date-range')?.dataset.calendarPending==='false'})").await?;
    // The real capture-phase calendar listener consumes outside clicks before
    // any Activity handler. The current file entity is static while open.
    // This is an outside-dismissal check, not a synthetic direct Rust callback
    // or a claim that the covered background is pointer-accessible.
    let attempt=wait_dom(d,&format!(r#"(()=>{{const e=document.querySelector('#workflow-activity-trigger-message .workflow-activity-file'),n=document.getElementById('workflow-management')?.__workflowManagement,b=document.getElementById('workflow-activity-date-range');if(!e||e.tagName!=='SPAN'||e.dataset.file!=={}||e.getAttribute('href')!==null||e.closest('a,button')||document.getElementById({})||!n||b?.dataset.calendarOpen!=='true'||b.dataset.calendarPending!=='false')return{{ok:false}};const before=n.channelRefusals||0,epoch=b.dataset.calendarEpoch;e.click();return{{ok:true,refusals:before,file:e.dataset.file,tag:e.tagName,href:e.getAttribute('href'),text:e.textContent,epoch,calendarOpen:true,activation:'outside-dom-click'}}}})()"#,json!(endpoint::LIST),json!(FIRST_LINK))).await?;
    let native=wait_dom(d,&format!("(()=>{{const n=document.getElementById('workflow-management')?.__workflowManagement,m=document.getElementById('workflow-activity-navigation-notice'),b=document.getElementById('workflow-activity-date-range');return{{ok:n?.channelRefusals==={}&&m?.textContent===''&&!document.getElementById('workflow-activity-calendar')&&b?.dataset.calendarOpen==='false'&&b.dataset.calendarPending==='false'&&Number(b.dataset.calendarEpoch)>Number({})&&!!document.getElementById({}),refusals:n?.channelRefusals,notice:m?.textContent,calendarOpen:!!document.getElementById('workflow-activity-calendar'),pending:b?.dataset.calendarPending,epoch:b?.dataset.calendarEpoch}}}})()",attempt["refusals"],attempt["epoch"],json!(FIRST_LINK))).await?;
    if snapshot(&s.peek()) != before
        || endpoint::snapshot(&s.peek()) != files
        || destination::snapshot(&s.peek()) != destination_before
    {
        return Err("Open calendar refusal changed original query or file requests".into());
    }
    // Close the remaining real Filter before the ordinary detail capture:
    // the foreground panel deliberately occludes background status text.
    native::reveal_control(d, FIRST_LINK).await?;
    let restored_labels = labels(d, false).await?;
    if snapshot(&s.peek()) != before
        || endpoint::snapshot(&s.peek()) != files
        || destination::snapshot(&s.peek()) != destination_before
    {
        return Err("Closing Filter after consumed click changed original custody".into());
    }
    Ok(
        json!({"mode":"calendar-outside-consumed","before":before,"filesBefore":files,"destinationBefore":destination_before,"originalLabels":original_labels,"attempt":attempt,"native":native,"restoredLabels":restored_labels,"filterClosedBeforeCapture":true,"unchangedRustState":true,"noFileRead":true,"noDestination":true}),
    )
}
pub(super) async fn calendar_end(d: &DesktopContext) -> Result<(), String> {
    wait_dom(d,"({ok:!document.getElementById('workflow-activity-calendar')&&document.getElementById('workflow-activity-date-range')?.dataset.calendarPending==='false'})").await?;
    native::reveal_control(d, FIRST_LINK).await?;
    wait_dom(d,"(()=>{document.getElementById('workflow-activity-navigation-notice').textContent='';return{ok:true}})()").await?;
    Ok(())
}
