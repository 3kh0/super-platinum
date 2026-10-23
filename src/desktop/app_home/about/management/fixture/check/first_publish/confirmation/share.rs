//! Original Share controls and independent send receipts; no controller calls.
use super::*;
use endpoint::confirmation::share as backend;
pub(crate) mod cancel;
pub(crate) mod dm;
pub(crate) mod drafts;
pub(crate) mod multiple;
pub(crate) mod person;
pub(crate) mod posting;
pub(crate) mod resend;
pub(crate) mod search;

pub(in super::super) fn requested() -> bool {
    backend::requested()
}
fn value(p: &Value) -> &Value {
    &last(p)["confirmationShare"]
}
pub(in super::super) fn snapshot(s: &ShellState) -> Value {
    let bag = s
        .core
        .workspaces
        .get("TWORKFLOW")
        .and_then(|w| w.messages.get(backend::destination()));
    let pending = s
        .pending_delivery
        .iter()
        .map(|(k, v)| (k.clone(), format!("{v:?}")))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut result = json!({"composer":s.core.composer,"threadComposer":s.core.thread_composer,
        "composerAttachments":s.core.composer_attachments.len(),"threadAttachments":s.core.thread_composer_attachments.len(),
        "threadRoot":s.thread_root,"threadGeneration":s.thread_generation,
        "drafts":s.core.drafts.items.iter().map(|(k,v)|json!({"key":k,"draft":v})).collect::<Vec<_>>(),
        "messages":bag.map(|v|&v.messages),"pending":bag.map(|v|&v.pending),"delivery":pending});
    if backend::startup::requested() {
        result["startupObservations"] = crate::app_home::autocomplete::observations::snapshot(s);
    }
    result
}
pub(in super::super) fn native_script(base: &str) -> String {
    let base = multiple::native_base(&search::native_base(&person::native_base(
        &dm::native_base(base),
    )));
    let script = r#"(()=>{
 const b=BASE,r=window.__firstPublishRegistry,id=e=>{if(!e)return null;if(!r.ids.has(e))r.ids.set(e,r.next++);return r.ids.get(e)},rect=e=>{const v=e.getBoundingClientRect();return{left:v.left,top:v.top,right:v.right,bottom:v.bottom,width:v.width,height:v.height}};
 const shown=e=>!!e?.isConnected&&e.getClientRects().length>0&&getComputedStyle(e).visibility==='visible';
 const plain=e=>e?{id:e.id,nodeId:id(e),connected:e.isConnected,value:e.value??null,start:e.selectionStart??null,end:e.selectionEnd??null,displayed:shown(e)}:null;
 const paint=e=>{e.scrollIntoView({block:'center',inline:'nearest',behavior:'instant'});const q=rect(e),x=q.left+q.width/2,y=q.top+q.height/2,h=document.elementFromPoint(x,y);return{...plain(e),text:e.textContent,tag:e.tagName,disabled:!!e.disabled,...(PERSON?{readOnly:!!e.readOnly}:{}),rect:q,hit:h===e||e.contains(h),visible:shown(e)&&q.width>0&&q.height>0&&q.left>=0&&q.top>=0&&q.right<=innerWidth+1&&q.bottom<=innerHeight+1&&(h===e||e.contains(h))}};
 const panel=document.getElementById('workflow-finish-share-pane'),root=document.getElementById('workflow-management'),saved=root?.__workflowManagement?.permissionsConfirmation?.share;
 const active=shown(panel),nodes={panel,select:document.getElementById('workflow-share-destination'),text:document.getElementById('workflow-share-message'),refresh:document.getElementById('workflow-share-refresh'),cancel:document.getElementById('workflow-share-cancel'),send:document.getElementById('workflow-share-send'),...(SEARCH?{query:document.getElementById('workflow-share-query')}:{})};
 const share=active?{nodeId:id(panel),generation:panel.dataset.shareGeneration,epoch:panel.dataset.shareEpoch,selected:panel.dataset.shareSelected,originalNodes:!!saved&&Object.keys(nodes).every(k=>saved[k]===nodes[k]),controls:Object.fromEntries(Object.entries(nodes).filter(([k])=>k!=='panel').map(([k,e])=>[k,paint(e)])),options:[...nodes.select.options].map(o=>({value:o.value,text:o.textContent,disabled:o.disabled})),notice:document.getElementById('workflow-share-notice')?.textContent??null,...(SEARCH?{queryRevision:panel.dataset.shareQueryRevision,searchStatus:document.getElementById('workflow-share-search-status')?paint(document.getElementById('workflow-share-search-status')):null}:{})}:null;
 if(active){b.outcomes=[];b.confirmationAdd=null;b.modal.pane='share';b.modal.activePane={...plain(panel),savedSame:root?.__workflowManagement?.permissionsConfirmation?.finishPane?.node===panel};b.ok=b.originalNodes===true&&b.modal.originalNodes===true&&b.noOverflow===true&&!b.composing&&b.controls.every(v=>v.visible&&v.hit)&&b.feedback.every(v=>v.visible&&v.hit)&&share.originalNodes&&Object.values(share.controls).every(v=>v.visible&&v.hit);}
 const timeline=document.getElementById('message-timeline');
 const rows=timeline?[...timeline.querySelectorAll('article[data-message-id]')].map(e=>({...paint(e),messageId:e.dataset.messageId,links:[...e.querySelectorAll('a[href]')].map(a=>({text:a.textContent,href:a.getAttribute('href')}))})):[];
 return{...b,share,shareComposers:{channel:plain(document.getElementById('channel-composer')),thread:plain(document.getElementById('thread-composer'))},shareTimeline:timeline?{nodeId:id(timeline),connected:timeline.isConnected,scope:timeline.dataset.timelineScope,rows}:null};
})()"#.replace("SEARCH",if backend::search::requested()||backend::multiple::requested(){"true"}else{"false"}).replace("PERSON",if backend::person::requested()||backend::search::requested()||backend::multiple::requested(){"true"}else{"false"}).replace("BASE",&base);
    multiple::native(&script)
}

async fn evaluate(desktop: &DesktopContext, script: &str) -> Result<Value, String> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    desktop
        .webview
        .evaluate_script_with_callback(script, move |v| {
            let _ = tx.send(v);
        })
        .map_err(|_| "Share native evaluation failed")?;
    let raw = tokio::time::timeout(Duration::from_secs(3), rx.recv())
        .await
        .map_err(|_| "Share evaluation timeout")?
        .ok_or("Share callback absent")?;
    let v: Value = serde_json::from_str(&raw).map_err(|_| "Share callback JSON invalid")?;
    if v["ok"] != true {
        return Err(format!("Original Share action refused: {v}"));
    }
    Ok(v)
}
async fn action(
    desktop: &DesktopContext,
    kind: &str,
    id: &str,
    value: &str,
) -> Result<Value, String> {
    let script=r#"(()=>{
 const kind=KIND,target=TARGET,value=VALUE,r=window.__firstPublishRegistry;
 const e=kind==='sidebar'?[...document.querySelectorAll('.sidebar button.channel')].find(e=>e.querySelector('.channel-name')?.textContent===target):document.getElementById(target);
 if(!e?.isConnected||e.disabled)return{ok:false,reason:'absent/disabled'};
 if(!r.ids.has(e))r.ids.set(e,r.next++);const nodeId=r.ids.get(e);
 const saved=document.getElementById('workflow-management')?.__workflowManagement?.permissionsConfirmation?.share;
 if(target.startsWith('workflow-share-')&&!Object.values(saved??{}).includes(e))return{ok:false,reason:'not original bound node'};
 e.scrollIntoView({block:'center',inline:'nearest',behavior:'instant'});const q=e.getBoundingClientRect(),x=q.left+q.width/2,y=q.top+q.height/2,h=document.elementFromPoint(x,y);
 if(q.width<=0||q.height<=0||q.left<0||q.top<0||q.right>innerWidth+1||q.bottom>innerHeight+1||(h!==e&&!e.contains(h)))return{ok:false,reason:'not exposed'};
 e.focus();let event=null;const type=kind==='text'?'input':kind==='select'?'change':'click',listener=v=>{event={type:v.type,isTrusted:v.isTrusted,targetNodeId:r.ids.get(v.target)}};e.addEventListener(type,listener,{once:true});
 if(kind==='text'){e.value=value;e.setSelectionRange(7,14);e.dispatchEvent(new Event('input',{bubbles:true}));}
 else if(kind==='select'){if(![...e.options].some(o=>o.value===value&&!o.disabled))return{ok:false};e.value=value;e.dispatchEvent(new Event('change',{bubbles:true}));}
 else e.click();e.removeEventListener(type,listener);
 return{ok:!!event,kind,nodeId,id:e.id,value:e.value??null,start:e.selectionStart??null,end:e.selectionEnd??null,event,hitPoint:{x,y},rect:{left:q.left,top:q.top,right:q.right,bottom:q.bottom,width:q.width,height:q.height}};
})()"#.replace("KIND",&json!(kind).to_string()).replace("TARGET",&json!(id).to_string()).replace("VALUE",&json!(value).to_string());
    evaluate(desktop, &script).await
}
async fn settled(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
) -> Result<Value, String> {
    until(
        state,
        visit,
        "original Share model/epoch settled",
        |p, _| value(p)["bound"] == true && value(p)["loading"] == false,
    )
    .await?;
    observe(state, desktop, visit).await
}
pub(super) async fn run(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    visit: u64,
    captures: &mut serde_json::Map<String, Value>,
    stages: &mut Vec<Value>,
) -> Result<Value, String> {
    if backend::multiple::requested() {
        return multiple::run(state, desktop, name, visit, captures, stages).await;
    }
    if backend::search::requested() {
        return search::run(state, desktop, name, visit, captures, stages).await;
    }
    if backend::person::requested() {
        return person::run(state, desktop, name, visit, captures, stages).await;
    }
    if backend::dm::requested()
        && matches!(
            backend::dm::case().as_str(),
            "deleted" | "mismatched-peer" | "held-cancel"
        )
    {
        return dm::run(state, desktop, name, visit, captures, stages).await;
    }
    let case = std::env::var(backend::CASE_ENV).unwrap_or_default();
    let before = observe(state, desktop, visit).await?;
    let activation = action(desktop, "click", "workflow-confirmation-share-open", "").await?;
    let opened = settled(state, desktop, visit).await?;
    stage(
        state,
        desktop,
        name,
        "share-open",
        visit,
        json!({"before":before,"activation":activation,"after":opened}),
        captures,
        stages,
    )
    .await?;
    let selected = action(
        desktop,
        "select",
        "workflow-share-destination",
        backend::dm::destination(),
    )
    .await?;
    until(
        state,
        visit,
        "original Share destination selected",
        |p, _| value(p)["selected"] == backend::dm::destination() && value(p)["bound"] == true,
    )
    .await?;
    let selection = settled(state, desktop, visit).await?;
    let input = action(desktop, "text", "workflow-share-message", backend::MESSAGE).await?;
    until(state, visit, "original Share input accepted", |p, _| {
        value(p)["text"] == backend::MESSAGE && value(p)["bound"] == true
    })
    .await?;
    let edited = settled(state, desktop, visit).await?;
    stage(state,desktop,name,"share-edited",visit,json!({"before":opened,"selectionActivation":selected,"selection":selection,"input":input,"after":edited}),captures,stages).await?;
    let original = backend::capture(&state.peek())?;
    let send = action(desktop, "click", "workflow-share-send", "").await?;
    if case == "destination-failure" {
        until(state, visit, "actual Share preflight refusal", |p, _| {
            value(p)["notice"].is_string() && value(p)["operation"].is_null()
        })
        .await?;
        let after = observe(state, desktop, visit).await?;
        stage(state,desktop,name,"share-unavailable",visit,json!({"before":edited,"activation":send,"after":after,"quiet":quiet(state,desktop,visit).await?}),captures,stages).await?;
        let cancel = action(desktop, "click", "workflow-share-cancel", "").await?;
        until(state, visit, "original Share cancelled", |p, _| {
            last(p)["closed"] == true && value(p)["active"] == false
        })
        .await?;
        stage(
            state,
            desktop,
            name,
            "share-cancelled",
            visit,
            json!({"before":after,"activation":cancel,"after":observe(state,desktop,visit).await?}),
            captures,
            stages,
        )
        .await?;
        return Ok(Value::Null);
    }
    if case == "held" {
        until(state, visit, "actual Share response held", |p, s| {
            value(p)["operation"]["outcome"] == "sending"
                && s["confirmationAdd"]["share"]["held"] == json!([1])
        })
        .await?;
        let held = observe(state, desktop, visit).await?;
        stage(state,desktop,name,"share-held",visit,json!({"before":edited,"activation":send,"after":held,"quiet":quiet(state,desktop,visit).await?}),captures,stages).await?;
        let cancel = action(desktop, "click", "workflow-share-cancel", "").await?;
        until(state, visit, "pending Share cancelled", |p, _| {
            last(p)["closed"] == true && value(p)["active"] == false
        })
        .await?;
        let cancelled = observe(state, desktop, visit).await?;
        stage(
            state,
            desktop,
            name,
            "share-cancelled",
            visit,
            json!({"before":held,"activation":cancel,"after":cancelled}),
            captures,
            stages,
        )
        .await?;
        let release = original.release(1)?;
        until(
            state,
            visit,
            "original cancelled Share result retained",
            |p, _| value(p)["operation"]["finished"] == true,
        )
        .await?;
        stage(state,desktop,name,"late-settled",visit,json!({"before":cancelled,"release":release,"after":observe(state,desktop,visit).await?}),captures,stages).await?;
    } else {
        until(state, visit, "Share queue outcome consumed", |p, _| {
            value(p)["operation"]["finished"] == true && last(p)["closed"] == true
        })
        .await?;
        stage(
            state,
            desktop,
            name,
            "share-settled",
            visit,
            json!({"before":edited,"activation":send,"after":observe(state,desktop,visit).await?}),
            captures,
            stages,
        )
        .await?;
    }
    let before = observe(state, desktop, visit).await?;
    let label = if backend::dm::requested() {
        backend::dm::LABEL
    } else {
        "review-destination"
    };
    let activation = action(desktop, "sidebar", label, "").await?;
    let expected_header = if backend::dm::requested() {
        backend::dm::LABEL
    } else {
        "#review-destination"
    };
    let script=r#"({ok:document.getElementById('channel-details-trigger')?.textContent===HEADER&&!!document.getElementById('message-timeline')&&!document.getElementById('workflow-finish-up')})"#.replace("HEADER",&json!(expected_header).to_string());
    let script = dm::native_base(&script);
    evidence::observed_wait(
        state,
        desktop,
        visit,
        "actual Share destination timeline",
        &script,
    )
    .await?;
    if backend::posting::requested() && !backend::posting::retry_requested() {
        until(
            state,
            visit,
            "original restricted refresh dispatched before destination capture",
            |p, s| {
                last(p)["confirmationShare"]["operation"]["postingRefresh"]
                    .as_array()
                    .and_then(|v| v.last())
                    .is_some_and(|r| r["outcome"] == "reading" && r["finished"] == false)
                    && s["confirmationAdd"]["share"]["posting"]["held"] == json!([1])
            },
        )
        .await?;
    }
    stage(state,desktop,name,"destination",visit,json!({"before":before,"activation":activation,"after":observe(state,desktop,visit).await?}),captures,stages).await?;
    Ok(Value::Null)
}
