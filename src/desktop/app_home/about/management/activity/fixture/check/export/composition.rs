//! Genuine member composition is refused before an export native lease starts.
use super::*;

fn composing(state: Signal<ShellState>) -> bool {
    let s = state.peek();
    s.app_home
        .about
        .management
        .surface
        .as_ref()
        .and_then(|h| crate::app_home::about::management::document(&s, h.id))
        .is_some_and(|d| d.activity.member_composing)
}
async fn settled_composition(state: Signal<ShellState>, expected: bool) -> Result<(), String> {
    for _ in 0..60 {
        if composing(state) == expected {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    Err("Original member composition did not settle".into())
}
async fn native(d: &DesktopContext, expected: bool) -> Result<Value, String> {
    wait_dom(d, &format!(r#"(()=>{{
const saved=window.__workflowExportComposition,s=document.getElementById('workflow-management')?.__workflowManagement,n=document.getElementById('workflow-activity-export-native-notice');
return{{ok:!!saved&&!!s&&s.composing==={}&&document.getElementById('workflow-activity-user-search')===saved.input&&document.getElementById('workflow-activity-export')===saved.button,sameNode:!!saved&&document.getElementById('workflow-activity-export')===saved.button,value:saved?.input.value,composing:!!s?.composing,refusals:s?.exportRefusals||0,notice:n?.textContent??''}};
}})()"#,json!(expected))).await
}
pub(super) async fn begin(s: Signal<ShellState>, d: &DesktopContext) -> Result<Value, String> {
    let model_before = snapshot(&s.peek());
    let ledger_before = ledger(s);
    wait_dom(d, r#"(()=>{
const input=document.getElementById('workflow-activity-user-search'),button=document.getElementById('workflow-activity-export');
if(!input||!button||button.disabled)return{ok:false};
window.__workflowExportComposition={input,button};
input.dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true,data:'x'}));return{ok:true};
})()"#).await?;
    settled_composition(s, true).await?;
    let native_before = native(d, true).await?;
    let click = wait_dom(d, r#"(()=>{
const saved=window.__workflowExportComposition,e=saved?.button;
if(!e||document.getElementById(e.id)!==e||e.disabled)return{ok:false};
e.scrollIntoView({block:'nearest'});const r=e.getBoundingClientRect(),style=getComputedStyle(e),h=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2),hit=h===e||e.contains(h);
let visible=r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1&&style.display!=='none'&&style.visibility==='visible';
for(let p=e.parentElement;p;p=p.parentElement){const q=p.getBoundingClientRect(),t=getComputedStyle(p);if(/auto|scroll|hidden|clip/.test(t.overflowY)&&(r.top<q.top-1||r.bottom>q.bottom+1))visible=false;if(/auto|scroll|hidden|clip/.test(t.overflowX)&&(r.left<q.left-1||r.right>q.right+1))visible=false;}
if(!visible||!hit||e.tagName!=='BUTTON'||e.textContent!=='Export CSV')return{ok:false};
const proof={ok:true,id:e.id,tag:e.tagName,text:e.textContent,sameNode:document.getElementById(e.id)===saved.button,visible,hit,disabled:e.disabled,rect:{left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height},viewport:[innerWidth,innerHeight]};
e.click();return proof;
})()"#).await?;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let native_after = native(d, true).await?;
    let model_after = snapshot(&s.peek());
    let ledger_after = ledger(s);
    if model_before != model_after
        || ledger_before != ledger_after
        || native_before != native_after
        || !composing(s)
    {
        return Err("Composing Export click changed original custody or feedback".into());
    }
    Ok(
        json!({"mode":"rust-composition-guard","composition":true,"stateComposing":true,"modelBefore":model_before,"modelAfter":model_after,"ledgerBefore":ledger_before,"ledgerAfter":ledger_after,"nativeBefore":native_before,"nativeAfter":native_after,"click":click,"observedMs":100}),
    )
}
pub(super) async fn finish(s: Signal<ShellState>, d: &DesktopContext) -> Result<Value, String> {
    wait_dom(d, r#"(()=>{
const saved=window.__workflowExportComposition;if(!saved||document.getElementById('workflow-activity-user-search')!==saved.input)return{ok:false};
saved.input.dispatchEvent(new CompositionEvent('compositionend',{bubbles:true,data:'x'}));return{ok:true};
})()"#).await?;
    settled_composition(s, false).await?;
    let native = native(d, false).await?;
    Ok(
        json!({"stateComposing":false,"native":native,"model":snapshot(&s.peek()),"ledger":ledger(s)}),
    )
}
