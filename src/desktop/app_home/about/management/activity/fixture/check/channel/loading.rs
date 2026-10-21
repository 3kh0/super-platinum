//! Bounded observation of actual pending feedback; never writes native guidance.
use super::*;

pub(super) async fn held(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
) -> Result<Value, String> {
    let before = snapshot(&state.peek());
    let endpoint_before = endpoint::snapshot(&state.peek());
    let script = include_str!("loading.js");
    let native_before = wait_dom(desktop, &script.replace("BEGIN", "true")).await?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let native_after = wait_dom(desktop, &script.replace("BEGIN", "false")).await?;
    if before != snapshot(&state.peek())
        || endpoint_before != endpoint::snapshot(&state.peek())
        || native_before != native_after
    {
        return Err("Held channel changed original native buffers, owner or request during quiet observation".into());
    }
    Ok(
        json!({"nativeBefore":native_before,"nativeAfter":native_after,
        "observedMs":500,"unchangedRustState":true,"unchangedEndpoint":true}),
    )
}

pub(super) async fn retired(desktop: &DesktopContext) -> Result<Value, String> {
    let mut proof = wait_dom(desktop, r#"(()=>{
 const old=window.__workflowHeldChannel,current=document.getElementById('workflow-management')?.__workflowManagement?.channelNavigationNotice;
 const visiblePending=[...document.querySelectorAll('#workflow-activity-navigation-notice')].filter(n=>n.isConnected&&n.textContent==='Opening conversation\u2026'&&n.getBoundingClientRect().width>0&&n.getBoundingClientRect().height>0).length;
 const oldPhase=old?.owner.phase,oldDetailConnected=!!old?.detail.isConnected,currentPending=current?.phase==='pending';
 return{ok:!!old&&oldPhase==='settled'&&!oldDetailConnected&&!visiblePending&&!currentPending,oldPhase,oldDetailConnected,visiblePending,currentPending};
})()"#).await?;
    proof.as_object_mut().unwrap().remove("ok");
    Ok(proof)
}
