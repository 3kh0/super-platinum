//! Canonical thread and Pins ingress, with original native surface custody.
use super::*;
use crate::app_home::about::management::entry::fixture as entry_fixture;

fn thread_case() -> bool {
    matches!(
        case().as_str(),
        "execution-thread" | "execution-focused" | "execution-thread-retirement"
    )
}
pub(super) async fn surface(d: &DesktopContext, seed: bool, hidden: bool) -> Result<Value, String> {
    let params = json!({"seed":seed,"hidden":hidden,"thread":thread_case(),"focused":case()=="execution-focused",
        "channelDraft":entry_fixture::COMPOSER,"threadDraft":entry_fixture::THREAD_COMPOSER,
        "label":entry_fixture::OPEN});
    wait_dom(d, &format!("({})({params})", include_str!("surfaces.js"))).await
}
pub(super) async fn action(d: &DesktopContext) -> Result<(), String> {
    wait_dom(d, r#"(()=>{const s=window.__executionSurfaceFixture,e=s?.action;if(!e?.isConnected||e.disabled||document.body.inert)return{ok:false};e.scrollIntoView({block:'nearest'});const r=e.getBoundingClientRect(),hit=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);if(hit!==e&&!e.contains(hit))return{ok:false};e.click();return{ok:true}})()"#).await?;
    Ok(())
}
async fn stage(
    s: Signal<ShellState>,
    d: &DesktopContext,
    name: &str,
    label: &str,
    rows: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    let hidden = !matches!(label, "source" | "returned");
    let native = surface(d, label == "source", hidden).await?;
    capture(
        s,
        d,
        name,
        label,
        json!({"source":entry_fixture::snapshot(&s.peek()),"nativeSurface":native}),
        rows,
        captures,
    )
    .await
}
pub(super) async fn prepare_thread(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
) -> Result<(), String> {
    if thread_case() {
        // Exercise normal native input/autosize before leaving a raw buffer one
        // edit ahead of Rust. A setter alone bypasses the input resize listener.
        let committed = format!("{}.", entry_fixture::THREAD_COMPOSER);
        input(desktop, "thread-composer", &committed).await?;
        for _ in 0..100 {
            if state.peek().core.thread_composer.text == committed {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        if state.peek().core.thread_composer.text != committed {
            return Err("Thread input did not reach its original composer".into());
        }
    }
    Ok(())
}
pub(super) async fn run(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    let readiness = readiness(name).await?;
    let mut stages = vec![];
    let mut captures = serde_json::Map::new();
    prepare_thread(state, desktop).await?;
    stage(state, desktop, name, "source", &mut stages, &mut captures).await?;
    action(desktop).await?;
    execution::opened(state, desktop).await?;
    stage(state, desktop, name, "opened", &mut stages, &mut captures).await?;
    click(desktop, "workflow-activity-back").await?;
    wait_dom(desktop,"({ok:!!document.getElementById('workflow-management-title')&&!document.getElementById('workflow-activity')})").await?;
    input(desktop, "workflow-management-title", TITLE).await?;
    input(desktop, "workflow-management-description", DESCRIPTION).await?;
    wait(state, "Exact surface-origin dirty metadata", exact).await?;
    stage(
        state,
        desktop,
        name,
        "metadata-draft",
        &mut stages,
        &mut captures,
    )
    .await?;
    click(desktop, "workflow-entry-back").await?;
    wait(state, "Original source restored", |v| {
        v["entry"]["active"] == false
    })
    .await?;
    stage(state, desktop, name, "returned", &mut stages, &mut captures).await?;
    action(desktop).await?;
    execution::opened(state, desktop).await?;
    wait(state, "Exact surface-origin draft reopened", exact).await?;
    stage(state, desktop, name, "reopened", &mut stages, &mut captures).await?;
    Ok(
        json!({"ok":true,"fixture":name,"case":case(),"readiness":readiness,
        "viewport":stages.last().unwrap()["geometry"]["viewport"],"actualMessageAction":true,
        "aboutConfiguration":false,"actualActivityEntry":true,"noLiveNetwork":true,
        "stages":stages,"captures":captures,"final":snapshot(&state.peek())}),
    )
}
