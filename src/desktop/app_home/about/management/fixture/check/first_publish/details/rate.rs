use super::*;
async fn blocked(scene: &Scene<'_>) -> Result<Value, String> {
    let before = observe(scene.state, scene.desktop, scene.visit).await?;
    let activation=evidence::observed_wait(scene.state,scene.desktop,scene.visit,"disabled original details Retry",r#"(()=>{const e=document.getElementById('channel-workflow-details-retry'),r=window.__firstPublishRegistry;if(!e?.disabled)return{ok:false};const nodeId=r.ids.get(e);e.click();return{ok:true,id:e.id,nodeId,disabled:e.disabled}})()"#).await?;
    let q = quiet(scene.state, scene.desktop, scene.visit).await?;
    let after = observe(scene.state, scene.desktop, scene.visit).await?;
    if before != after {
        return Err("Disabled details Retry dispatched work".into());
    }
    Ok(json!({"before":before,"activation":activation,"after":after,"quiet":q}))
}
pub(super) async fn run(scene: &mut Scene<'_>) -> Result<(), String> {
    let opening = open(scene.state, scene.desktop, scene.visit, A, "failed").await?;
    let exercise = blocked(scene).await?;
    scene
        .stage(
            "rate-limited",
            json!({"opening":opening,"blocked":exercise}),
        )
        .await?;
    scene.close("closed").await?;
    let opening = open(scene.state, scene.desktop, scene.visit, A, "idle").await?;
    let exercise = blocked(scene).await?;
    scene
        .stage(
            "reopened-blocked",
            json!({"opening":opening,"blocked":exercise}),
        )
        .await?;
    let before = observe(scene.state, scene.desktop, scene.visit).await?;
    let started = std::time::Instant::now();
    let mut expired = false;
    for _ in 0..200 {
        if crate::channel_workflows::details::snapshot(&scene.state.peek())["cooldown"]["blocked"]
            == false
        {
            expired = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    if !expired {
        return Err("Original details Retry-After did not expire".into());
    }
    evidence::observed_wait(
        scene.state,
        scene.desktop,
        scene.visit,
        "details expired button",
        r#"({ok:document.getElementById('channel-workflow-details-retry')?.disabled===false})"#,
    )
    .await?;
    let after = observe(scene.state, scene.desktop, scene.visit).await?;
    let q = quiet(scene.state, scene.desktop, scene.visit).await?;
    if before["server"] != after["server"] || after["model"]["workflowDetails"]["serial"] != 0 {
        return Err("Details expiry replayed acquisition".into());
    }
    scene.stage("expired",json!({"before":before,"after":after,"quiet":q,"waitedMs":started.elapsed().as_millis() as u64})).await?;
    scene.retry("recovered", "ready").await?;
    scene.close("closed-final").await
}
