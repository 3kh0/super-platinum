//! Native CHANNEL occurrence navigation and real About reentry.
use super::super::channel as endpoint;
use super::*;
mod composition;
mod loading;
mod shared_frozen;
mod special;
const KNOWN_BUTTON: &str = "workflow-activity-channel-trigger-1";
const MISSING_BUTTON: &str = "workflow-activity-channel-step-0-1";
const REFUSAL: &str =
    "Finish editing the current text before opening this conversation. Your draft is unchanged.";

async fn channel_wait(
    state: Signal<ShellState>,
    label: &str,
    predicate: impl Fn(&Value) -> bool,
) -> Result<Value, String> {
    for _ in 0..100 {
        let v = endpoint::snapshot(&state.peek());
        if predicate(&v) {
            return Ok(v);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!("{label}: {}", endpoint::snapshot(&state.peek())))
}
async fn sidebar(desktop: &DesktopContext, model: &Value, measure: bool) -> Result<Value, String> {
    wait_dom(desktop,&format!(r#"(()=>{{
        const expected={},measure={},nodes=[...document.querySelectorAll('.sidebar .channel-list button.channel')];
        const rows=nodes.map(e=>({{name:e.querySelector('.channel-name')?.textContent,active:e.classList.contains('active')}}));
        const headings=[...document.querySelectorAll('.sidebar .channel-list .section-toggle span')].map(e=>e.textContent);
        const exact=rows.length===expected.rows.length&&rows.every((r,i)=>r.name===expected.rows[i].name&&r.active===expected.rows[i].active)&&headings.length===expected.sections.length&&headings.every((text,i)=>text===expected.sections[i].title);
        const activePosition=rows.findIndex(r=>r.active),mapped=expected.rows[activePosition],target=nodes[activePosition];
        if(!exact||rows.filter(r=>r.active).length!==1||mapped?.id!==expected.activeId||mapped?.index!==expected.activeIndex)return{{ok:false,rows,headings,activePosition,expected}};
        if(measure)target.scrollIntoView({{block:'nearest'}});
        const r=target.getBoundingClientRect(),style=getComputedStyle(target),hit=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);
        let visible=r.width>0&&r.height>0&&r.top>=0&&r.left>=0&&r.bottom<=innerHeight+1&&r.right<=innerWidth+1&&style.display!=='none'&&style.visibility==='visible'&&(hit===target||target.contains(hit));
        for(let p=target.parentElement;p;p=p.parentElement){{const b=p.getBoundingClientRect(),s=getComputedStyle(p);if(/auto|scroll|hidden|clip/.test(s.overflowY)&&(r.top<b.top-1||r.bottom>b.bottom+1))visible=false;}}
        return{{ok:!measure||visible,rows,headings,activePosition,activeIndex:mapped.index,activeId:mapped.id,measured:measure,visible,hit:hit===target||target.contains(hit),rect:{{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}}}};
    }})()"#,model,json!(measure))).await
}
async fn channel_labels(desktop: &DesktopContext, fetched: bool) -> Result<Value, String> {
    if endpoint::shared_frozen::requested() {
        return shared_frozen::labels(desktop, fetched).await;
    }
    if case() == "channel-special" {
        return special::labels(desktop, fetched).await;
    }
    let script = r#"(()=>{
        const fetched=FETCHED,tagPath=TAG_PATH;
        const ids=['workflow-activity-channel-trigger-1','workflow-activity-channel-trigger-3','workflow-activity-channel-step-0-1'];
        const rect=e=>{const r=e.getBoundingClientRect();return{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}};
        const visible=e=>{if(!e)return false;const r=rect(e),s=getComputedStyle(e),h=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);if(!(r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1&&s.visibility==='visible'&&s.display!=='none'&&(h===e||e.contains(h))))return false;for(let p=e.parentElement;p;p=p.parentElement){const b=rect(p),t=getComputedStyle(p);if(/auto|scroll|hidden|clip/.test(t.overflowY)&&(r.top<b.top-1||r.bottom>b.bottom+1))return false;}return true;};
        const rows=ids.map((id,i)=>{
            const e=document.getElementById(id);if(!e)return{id,ok:false};e.scrollIntoView({block:'nearest'});
            const label=e.querySelector('.workflow-activity-channel-label'),name=e.querySelector('.workflow-activity-channel-name'),icons=[...e.querySelectorAll('.workflow-activity-channel-icon')],svg=icons[0];
            const expected=i<2?'activity-known':fetched?'activity-fetched':'Channel unavailable',known=i<2||fetched;
            const icon=svg?{tag:svg.tagName.toLowerCase(),ariaHidden:svg.getAttribute('aria-hidden'),viewBox:svg.getAttribute('viewBox'),material:svg.classList.contains('material-icon'),pathMatches:svg.querySelectorAll('path').length===1&&svg.querySelector('path')?.getAttribute('d')===tagPath,visible:visible(svg),rect:rect(svg)}:null;
            const nameRect=name?rect(name):null,inline=!!(icon&&nameRect&&icon.rect.right<=nameRect.left+1&&Math.abs((icon.rect.top+icon.rect.bottom-nameRect.top-nameRect.bottom)/2)<=3);
            const shape=e.tagName==='BUTTON'&&label?.tagName==='SPAN'&&name?.tagName==='SPAN'&&label.textContent===expected&&name.textContent===expected&&e.textContent===expected&&e.querySelectorAll('.workflow-activity-channel-label').length===1&&e.querySelectorAll('.workflow-activity-channel-name').length===1&&e.querySelectorAll('svg').length===(known?1:0)&&icons.length===(known?1:0);
            return{id,channel:e.dataset.channel,text:e.textContent,tag:e.tagName,name:name?.textContent,labelTag:label?.tagName,nameTag:name?.tagName,iconCount:icons.length,svgCount:e.querySelectorAll('svg').length,icon,nameRect,buttonRect:rect(e),visible:visible(e)&&visible(name),inline:known?inline:null,scrollSample:true,ok:shape&&visible(e)&&visible(name)&&(!known||(icon.tag==='svg'&&icon.ariaHidden==='true'&&icon.viewBox==='0 -960 960 960'&&icon.material&&icon.pathMatches&&icon.visible&&inline))};
        });return{ok:rows.every(r=>r.ok),fetched,measurement:'individual-scroll-samples',rows};
    })()"#
    .replace("FETCHED", &json!(fetched).to_string())
    .replace("TAG_PATH", &json!(crate::icons::Icon::Tag.path()).to_string());
    wait_dom(desktop, &script).await
}
async fn stage(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    label: &str,
    exercise: Value,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    let mut proof = json!({"channel":endpoint::snapshot(&state.peek()),"exercise":exercise});
    if matches!(
        label,
        "detail" | "filter-refused" | "search-refused" | "malformed" | "offline" | "held"
    ) {
        proof["channelLabels"] = channel_labels(desktop, false).await?;
        if endpoint::shared_frozen::requested() {
            proof["channel"] = endpoint::snapshot(&state.peek());
        }
    }
    proof["sidebarNative"] = sidebar(
        desktop,
        &proof["channel"]["sidebar"],
        matches!(label, "known" | "retried" | "archived" | "private-archived")
            || shared_frozen::target(label).is_some(),
    )
    .await?;
    let missing = proof["channel"]["sidebar"]["rows"]
        .as_array()
        .is_some_and(|rows| rows.iter().any(|row| row["id"] == endpoint::MISSING));
    if missing != matches!(label, "retried" | "final-return") {
        return Err(format!(
            "Fetched sidebar row appeared before accepted response or disappeared afterwards: {label}"
        ));
    }
    if !matches!(label, "known" | "retried" | "archived" | "private-archived")
        && shared_frozen::target(label).is_none()
    {
        return capture(state, desktop, name, label, proof, stages, captures).await;
    }
    let expected = if let Some((name, _)) = shared_frozen::target(label) {
        name
    } else if label == "known" {
        "activity-known"
    } else {
        "activity-fetched"
    };
    let (timeline, composer) = {
        let shell = state.peek();
        (
            shell.timeline_scope(),
            crate::view::composer::channel_field_scope(&shell),
        )
    };
    proof["conversationOwner"] = json!({"timeline":timeline,"composer":composer});
    let archived = matches!(label, "archived" | "private-archived")
        || shared_frozen::target(label).is_some_and(|(_, archived)| archived);
    let mut geometry = if archived {
        proof["conversationOwner"]["composer"] = Value::Null;
        if shared_frozen::target(label).is_some() {
            shared_frozen::archived_target(desktop, expected, &timeline).await?
        } else {
            special::target(desktop, label, &timeline).await?
        }
    } else {
        wait_dom(desktop,&format!(r#"(()=>{{
        const timeline=document.getElementById('message-timeline'),header=document.querySelector('.conversation-header .header-name'),composer=document.getElementById('channel-composer');
        const rect=e=>{{const r=e.getBoundingClientRect();return{{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}}}};
        const visible=e=>{{const r=rect(e),s=getComputedStyle(e),h=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);return r.width>0&&r.height>0&&r.top>=0&&r.left>=0&&r.bottom<=innerHeight+1&&r.right<=innerWidth+1&&s.display!=='none'&&s.visibility==='visible'&&(h===e||e.contains(h));}};
        if(!timeline||!header||!composer||document.getElementById('workflow-management'))return{{ok:false}};
        const controls=[header,timeline,composer].map(e=>({{id:e.id,visible:visible(e),rect:rect(e)}}));
        const timelineScope=timeline.dataset.timelineScope,composerScope=composer.dataset.draftScope,headerText=header.textContent,placeholder=composer.placeholder;
        return{{ok:timelineScope==={}&&composerScope==={}&&headerText==={}&&placeholder==={}&&controls.every(e=>e.visible),viewport:[innerWidth,innerHeight],noOverflow:document.documentElement.scrollWidth<=innerWidth+1,conversation:true,timelineScope,composerScope,headerText,placeholder,controls,notices:[]}};
    }})()"#,json!(timeline),json!(composer),json!(format!("#{expected}")),json!(format!("Message #{expected}")))).await?
    };
    let directory = std::env::var_os("SUPER_PLATINUM_UI_CAPTURE_DIR")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or("Absolute capture directory required")?;
    let path = directory.join(format!("{name}-{label}.png"));
    let inspect = path.clone();
    if tokio::task::spawn_blocking(move || inspect.try_exists())
        .await
        .map_err(|_| "Capture inspection")?
        .map_err(|_| "Capture inspection")?
    {
        return Err("Capture must be fresh".into());
    }
    eprintln!(
        "workflow-activity-capture: {}",
        json!({"stage":label,"edge":"before"})
    );
    let (path, width, height) =
        crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), desktop).await?;
    geometry["pngDimensions"] = json!([width, height]);
    captures.insert(label.into(), json!(path.to_string_lossy()));
    stages.push(
        json!({"stage":label,"model":snapshot(&state.peek()),"geometry":geometry,"proof":proof}),
    );
    eprintln!(
        "workflow-activity-capture: {}",
        json!({"stage":label,"edge":"after"})
    );
    Ok(())
}
async fn detail(state: Signal<ShellState>, desktop: &DesktopContext) -> Result<Value, String> {
    let surface = wait_dom(
        desktop,
        "(()=>{const detail=document.getElementById('workflow-activity-detail'),close=document.getElementById('workflow-activity-close-detail'),row=document.getElementById('workflow-activity-run-E_REVIEW');return{ok:!!((detail&&close)||row),retained:!!(detail&&close)}})()",
    )
    .await?;
    if surface["retained"] != true {
        click(desktop, "workflow-activity-run-E_REVIEW").await?;
    }
    wait(state, "detail admitted", |v| {
        v["activity"]["active"] == true
            && v["activity"]["detail"]["workflow_event"]["workflow_execution_id"] == FIRST
            && v["activity"]["detailRow"]["workflow_execution_id"] == FIRST
            && v["metadata"]["busy"] == false
    })
    .await?;
    if endpoint::shared_frozen::requested() {
        return channel_labels(desktop, false).await;
    }
    let script = r#"(()=>{
        const expected=[['workflow-activity-channel-trigger-1','CACTKNOWN01'],['workflow-activity-channel-trigger-3','CACTKNOWN01'],['workflow-activity-channel-step-0-1','CACTMISS01']];
        if(SPECIAL)expected.push(['workflow-activity-channel-step-1-1','CACTARCH01'],['workflow-activity-channel-step-1-3','CACTPRARCH01']);
        const rows=expected.map(([id])=>{const e=document.getElementById(id);return{id,channel:e?.dataset.channel,text:e?.textContent,tag:e?.tagName}});
        return{ok:rows.every((r,i)=>r.tag==='BUTTON'&&r.channel===expected[i][1]),rows};
    })()"#.replace("SPECIAL", &json!(case() == "channel-special").to_string());
    wait_dom(desktop, &script).await
}
async fn refused(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    kind: &str,
) -> Result<Value, String> {
    native::reveal_filters(desktop).await?;
    // Finish the actual popup transaction before the uncommitted native probe.
    native::reveal_control(desktop, KNOWN_BUTTON).await?;
    if kind == "composition-refused" {
        return composition::begin(state, desktop).await;
    }
    let before = snapshot(&state.peek());
    let channel = endpoint::snapshot(&state.peek());
    let (id, buffer) = if kind == "filter-refused" {
        ("workflow-activity-status", "ERROR")
    } else {
        ("workflow-activity-user-search", "Uncommitted native search")
    };
    let probe = if kind == "composition-refused" {
        "document.getElementById('workflow-activity-user-search').dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true,data:'draft'}));".to_owned()
    } else {
        format!(
            "const e=document.getElementById({});Object.getOwnPropertyDescriptor(e.tagName==='SELECT'?HTMLSelectElement.prototype:HTMLInputElement.prototype,'value').set.call(e,{});",
            json!(id),
            json!(buffer)
        )
    };
    let attempt=wait_dom(desktop,&format!(r#"(()=>{{const root=document.getElementById('workflow-management'),s=root?.__workflowManagement;if(!s)return{{ok:false}};{probe}return{{ok:true,refusals:s.channelRefusals||0,composing:s.composing,value:document.getElementById({})?.value}}}})()"#,json!(id))).await?;
    click(desktop, KNOWN_BUTTON).await?;
    let after_dom=wait_dom(desktop,&format!(r#"(()=>{{const s=document.getElementById('workflow-management')?.__workflowManagement,n=document.getElementById('workflow-activity-navigation-notice');return{{ok:s?.channelRefusals==={}&&n?.textContent==={},refusals:s?.channelRefusals,notice:n?.textContent,value:document.getElementById({})?.value,composing:s?.composing}}}})()"#,attempt["refusals"].as_u64().unwrap()+1,json!(REFUSAL),json!(id))).await?;
    if snapshot(&state.peek()) != before || endpoint::snapshot(&state.peek()) != channel {
        return Err("Native refusal changed Rust state or endpoint ledger".into());
    }
    Ok(
        json!({"kind":kind,"attempt":attempt,"native":after_dom,"unchangedRustState":true,"before":channel,"after":endpoint::snapshot(&state.peek())}),
    )
}
async fn restore(desktop: &DesktopContext, kind: &str) -> Result<(), String> {
    let script = match kind {
        "filter-refused" => {
            "Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(document.getElementById('workflow-activity-status'),'');"
        }
        "search-refused" => {
            "Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(document.getElementById('workflow-activity-user-search'),'');"
        }
        _ => {
            "document.getElementById('workflow-activity-user-search').dispatchEvent(new CompositionEvent('compositionend',{bubbles:true,data:'draft'}));"
        }
    };
    wait_dom(desktop,&format!("(()=>{{{script}document.getElementById('workflow-activity-navigation-notice').textContent='';return{{ok:true}}}})()")).await.map(|_|())
}
async fn return_document(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    before: &Value,
) -> Result<Value, String> {
    // Use the original app DM and actual About tab. No installer or state replacement.
    wait_dom(desktop,r#"(()=>{const e=[...document.querySelectorAll('.sidebar button.channel')].find(e=>e.querySelector('.channel-name')?.textContent==='Review requests');if(!e)return{ok:false};e.scrollIntoView({block:'nearest'});e.click();return{ok:true}})()"#).await?;
    channel_wait(state, "original app DM selected", |v| {
        v["selected"] == v["ledger"]["original"]
    })
    .await?;
    let scope = state.peek().timeline_scope();
    let tab = wait_dom(desktop,&format!(r#"(()=>{{const timeline=document.getElementById('message-timeline'),selected=document.querySelector('.sidebar button.channel.active .channel-name'),tabs=document.querySelector('.conversation > div > .channel-tabs'),e=tabs&&[...tabs.querySelectorAll('button')].find(e=>e.textContent.trim()==='About');if(timeline?.dataset.timelineScope!=={}||selected?.textContent!=='Review requests'||!e||e.disabled||e.getAttribute('aria-pressed')!=='false')return{{ok:false,scope:timeline?.dataset.timelineScope,selected:selected?.textContent}};e.scrollIntoView({{block:'nearest'}});const r=e.getBoundingClientRect(),hit=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);if(hit!==e&&!e.contains(hit))return{{ok:false}};e.click();return{{ok:true,scope:timeline.dataset.timelineScope,selected:selected.textContent}}}})()"#,json!(scope))).await?;
    wait_dom(desktop,r#"(()=>{const e=[...document.querySelectorAll('.conversation > div > .channel-tabs button')].find(e=>e.textContent.trim()==='About');return{ok:e?.getAttribute('aria-pressed')==='true'}})()"#).await?;
    wait_dom(desktop,"({ok:!!document.getElementById('workflow-configuration')&&!document.getElementById('workflow-configuration').disabled})").await?;
    click(desktop, "workflow-configuration").await?;
    wait(state, "reattached dirty document", |v| {
        exact(v) && v["metadata"]["busy"] == false
    })
    .await?;
    // Reentry preserves the active Activity route; use its real back button.
    wait_dom(
        desktop,
        "({ok:!!document.getElementById('workflow-activity-back')})",
    )
    .await?;
    // End the Signal read before native evaluation can drive runtime writes.
    let missing_present = endpoint::snapshot(&state.peek())["missingPresent"] == true;
    let reentry_labels = channel_labels(desktop, missing_present).await?;
    click(desktop, "workflow-activity-back").await?;
    wait_dom(desktop,&format!("(()=>{{const e=document.getElementById('workflow-management-title');return{{ok:e?.value==={}}}}})()",json!(TITLE))).await?;
    let after = endpoint::snapshot(&state.peek());
    if after["documents"] != before["documents"]
        || after["aboutVisit"] == before["aboutVisit"]
        || after["managementVisit"] == before["managementVisit"]
    {
        return Err(format!(
            "Reentry did not reattach same draft with fresh visits: before={before}, after={after}"
        ));
    }
    Ok(
        json!({"before":before,"after":after,"nativeAboutClick":tab,"sameDocument":true,"freshAbout":true,"freshPolicy":true,"reentryLabels":reentry_labels}),
    )
}
pub(super) async fn run(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    if endpoint::shared_frozen::requested() {
        endpoint::shared_frozen::seed(state).await?;
    }
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
    wait(state, "metadata loaded", |v| {
        v["metadata"]["route"] == "editor" && v["metadata"]["busy"] == false
    })
    .await?;
    input(desktop, "workflow-management-title", TITLE).await?;
    input(desktop, "workflow-management-description", DESCRIPTION).await?;
    wait(state, "dirty metadata", exact).await?;
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
    let proof = detail(state, desktop).await?;
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
    for kind in ["filter-refused", "search-refused", "composition-refused"] {
        let proof = refused(state, desktop, kind).await?;
        stage(
            state,
            desktop,
            name,
            kind,
            proof,
            &mut stages,
            &mut captures,
        )
        .await?;
        if kind == "composition-refused" {
            let restored = composition::finish(state, desktop).await?;
            let exercise =
                &mut stages.last_mut().ok_or("Missing composition stage")?["proof"]["exercise"];
            if restored["model"] != exercise["before"]
                || restored["channel"] != exercise["channelBefore"]
            {
                return Err("Composition restoration changed original model or endpoint".into());
            }
            exercise["restored"] = restored;
        } else {
            restore(desktop, kind).await?;
        }
    }
    let before = endpoint::snapshot(&state.peek());
    click(desktop, "workflow-activity-channel-trigger-3").await?;
    channel_wait(state, "known channel selected", |v| {
        v["selected"] == endpoint::KNOWN
    })
    .await?;
    stage(
        state,
        desktop,
        name,
        "known",
        json!({"button":"workflow-activity-channel-trigger-3","before":before}),
        &mut stages,
        &mut captures,
    )
    .await?;
    let proof = return_document(state, desktop, &before).await?;
    stage(
        state,
        desktop,
        name,
        "returned",
        proof,
        &mut stages,
        &mut captures,
    )
    .await?;
    if case() == "channel-special" {
        special::navigate(state, desktop, name, &mut stages, &mut captures).await?;
    }
    if endpoint::shared_frozen::requested() {
        shared_frozen::navigate(state, desktop, name, &mut stages, &mut captures).await?;
    }
    click(desktop, "workflow-management-activity").await?;
    detail(state, desktop).await?;
    for (label, mode) in [
        ("malformed", endpoint::Mode::Malformed),
        ("offline", endpoint::Mode::Offline),
    ] {
        endpoint::mode(&state.peek(), mode).map_err(|e| format!("{label}: {e}"))?;
        let before = endpoint::snapshot(&state.peek());
        let model_before = snapshot(&state.peek());
        let count = before["ledger"]["completed"].as_u64().unwrap();
        click(desktop, MISSING_BUTTON).await?;
        channel_wait(state, "missing channel failure", |v| {
            v["ledger"]["completed"] == count + 1
        })
        .await?;
        tokio::time::sleep(Duration::from_millis(300)).await;
        let expected = if label == "malformed" {
            "Could not open this conversation. It may no longer be accessible."
        } else {
            "Connect to Slack to open this conversation."
        };
        let native=wait_dom(desktop,&format!(r#"(()=>{{
            const n=document.getElementById('workflow-activity-navigation-notice'),globalToastCount=document.querySelectorAll('#status-toast').length;
            if(!n)return{{ok:false,globalToastCount}};
            const r=n.getBoundingClientRect(),style=getComputedStyle(n),h=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);
            let visible=r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1&&style.display!=='none'&&style.visibility==='visible'&&(h===n||n.contains(h));
            for(let p=n.parentElement;p;p=p.parentElement){{const b=p.getBoundingClientRect(),s=getComputedStyle(p);if(/auto|scroll|hidden|clip/.test(s.overflowY)&&(r.top<b.top-1||r.bottom>b.bottom+1))visible=false;}}
            const outsideScroll=!n.closest('.workflow-activity-content'),detail=!!document.getElementById('workflow-activity-detail');
            return{{ok:n.textContent==={}&&globalToastCount===0&&visible&&outsideScroll&&detail,notice:n.textContent,globalToastCount,visible,outsideScroll,detail,rect:{{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}}}};
        }})()"#,json!(expected))).await?;
        let after = endpoint::snapshot(&state.peek());
        let model_after = snapshot(&state.peek());
        if model_before != model_after
            || [
                "documents",
                "selected",
                "generation",
                "aboutVisit",
                "managementVisit",
                "missingPresent",
            ]
            .iter()
            .any(|key| before[*key] != after[*key])
        {
            return Err("Channel failure changed retained document or navigation source".into());
        }
        let proof = json!({"native":native,"before":before,"after":after,"modelBefore":model_before,"modelAfter":model_after});
        stage(
            state,
            desktop,
            name,
            label,
            proof,
            &mut stages,
            &mut captures,
        )
        .await?;
    }
    endpoint::mode(&state.peek(), endpoint::Mode::Held)?;
    click(desktop, MISSING_BUTTON).await?;
    channel_wait(state, "held channel read", |v| v["ledger"]["held"] == true).await?;
    let held = loading::held(state, desktop).await?;
    stage(
        state,
        desktop,
        name,
        "held",
        held,
        &mut stages,
        &mut captures,
    )
    .await?;
    let held_success = case() == "channel-held-success";
    let (before, held_completion) = if held_success {
        let before = endpoint::snapshot(&state.peek());
        let completion = endpoint::release(&state.peek())?;
        if completion["delivered"] != true
            || completion["cancelledBeforeReply"] != false
            || completion["serial"] != 3
            || completion["requestCount"] != 3
        {
            return Err(format!(
                "Original held channel completion changed: {completion}"
            ));
        }
        (before, Some(completion))
    } else {
        click(desktop, "workflow-activity-close-detail").await?;
        channel_wait(state, "detail retired pending read", |v| {
            v["ledger"]["cancelled"] == 1
        })
        .await?;
        let mut release = endpoint::release(&state.peek())?;
        if release["cancelledBeforeReply"] != true {
            return Err("Retired channel response was delivered".into());
        }
        release["cleanup"] = loading::retired(desktop).await?;
        stage(
            state,
            desktop,
            name,
            "retired",
            release,
            &mut stages,
            &mut captures,
        )
        .await?;
        detail(state, desktop).await?;
        let before = endpoint::snapshot(&state.peek());
        click(desktop, MISSING_BUTTON).await?;
        (before, None)
    };
    channel_wait(state, "retry channel selected", |v| {
        v["selected"] == endpoint::MISSING && v["missingPresent"] == true
    })
    .await?;
    let mut exercise = json!({"before":before});
    if let Some(completion) = held_completion {
        let actual = endpoint::snapshot(&state.peek());
        if actual["ledger"]["requests"].as_array().map(Vec::len) != Some(3)
            || actual["ledger"]["completed"] != 3
            || actual["ledger"]["cancelled"] != 0
            || actual["ledger"]["held"] != false
        {
            return Err(
                "Held success performed another channel read or cancelled its receiver".into(),
            );
        }
        exercise["heldCompletion"] = completion;
    }
    stage(
        state,
        desktop,
        name,
        "retried",
        exercise,
        &mut stages,
        &mut captures,
    )
    .await?;
    let proof = return_document(state, desktop, &before).await?;
    stage(
        state,
        desktop,
        name,
        "final-return",
        proof,
        &mut stages,
        &mut captures,
    )
    .await?;
    Ok(
        json!({"ok":true,"fixture":name,"case":case(),"readiness":readiness,"viewport":stages.last().unwrap()["geometry"]["viewport"],"aboutConfiguration":true,"actualActivityEntry":true,"noLiveNetwork":true,"stages":stages,"captures":captures,"final":snapshot(&state.peek())}),
    )
}
