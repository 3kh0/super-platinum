//! Actual 429 cooldown, blocked controls, remount and explicit retry.
use super::*;
async fn blocked(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
) -> Result<Value, String> {
    let before = observe(state, desktop, visit).await?;
    let activation=wait_dom(desktop,r#"(()=>{const ids=['channel-workflows-refresh','channel-workflows-retry'],r=window.__firstPublishRegistry,nodes=ids.map(id=>document.getElementById(id));if(nodes.some(e=>!e||!e.disabled))return{ok:false};const controls=nodes.map(e=>({id:e.id,nodeId:r.ids.get(e),disabled:e.disabled}));for(const e of nodes)e.click();return{ok:true,controls}})()"#).await?;
    let q = quiet(state, desktop, visit).await?;
    let after = observe(state, desktop, visit).await?;
    if before != after {
        return Err(format!(
            "Disabled cooldown controls dispatched or changed state: before={before} after={after}"
        ));
    }
    Ok(json!({"before":before,"activation":activation,"after":after,"quiet":q}))
}
pub(super) async fn run(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    visit: u64,
    admission: Value,
    captures: &mut serde_json::Map<String, Value>,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    settled(desktop, "failed").await?;
    let original = observe(state, desktop, visit).await?;
    if original["native"]["workflows"]["product"]["cooldown"]["blocked"] != true {
        return Err("Actual429 did not retain cooldown".into());
    }
    let exercise = blocked(state, desktop, visit).await?;
    stage(
        state,
        desktop,
        name,
        "rate-limited",
        visit,
        json!({"admission":admission,"blocked":exercise}),
        captures,
        stages,
    )
    .await?;
    let before = observe(state, desktop, visit).await?;
    tab(desktop, "Messages").await?;
    evidence::observed_wait(state,desktop,visit,"cooldown actual Messages switch",r#"({ok:!document.getElementById('channel-workflows-panel')&&!!document.getElementById('message-timeline')&&!!document.getElementById('channel-composer')})"#).await?;
    let after = observe(state, desktop, visit).await?;
    if before["server"] != after["server"] {
        return Err("Tab unmount sent cooldown work".into());
    }
    stage(
        state,
        desktop,
        name,
        "switched",
        visit,
        json!({"before":before,"after":after}),
        captures,
        stages,
    )
    .await?;
    tab(desktop, "Workflows").await?;
    wait_dom(desktop,r#"(()=>{let p;try{p=JSON.parse(document.getElementById('channel-workflows-panel')?.dataset.fixtureReadback)}catch{return{ok:false}}return{ok:p.current===true&&p.serial===0&&p.status==='idle'&&p.cooldown?.blocked===true}})()"#).await?;
    let exercise = blocked(state, desktop, visit).await?;
    stage(
        state,
        desktop,
        name,
        "reentered-blocked",
        visit,
        json!({"blocked":exercise}),
        captures,
        stages,
    )
    .await?;
    let before = observe(state, desktop, visit).await?;
    let started = std::time::Instant::now();
    let mut expired = false;
    for _ in 0..200 {
        let value=wait_dom(desktop,r#"(()=>{let p;try{p=JSON.parse(document.getElementById('channel-workflows-panel')?.dataset.fixtureReadback)}catch{return{ok:false}}return{ok:!!p,current:p.current,blocked:p.cooldown?.blocked,serial:p.serial,status:p.status}})()"#).await?;
        if value["current"] == true && value["blocked"] == false {
            expired = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    if !expired {
        return Err("Actual Retry-After did not expire within20seconds".into());
    }
    let waited = started.elapsed().as_millis() as u64;
    let after = observe(state, desktop, visit).await?;
    let q = quiet(state, desktop, visit).await?;
    if before["server"] != after["server"] || after["native"]["workflows"]["product"]["serial"] != 0
    {
        return Err("Cooldown expiry automatically replayed".into());
    }
    stage(
        state,
        desktop,
        name,
        "expired",
        visit,
        json!({"before":before,"after":after,"waitedMs":waited,"quiet":q}),
        captures,
        stages,
    )
    .await?;
    let before = observe(state, desktop, visit).await?;
    click(desktop, "channel-workflows-retry").await?;
    settled(desktop, "ready").await?;
    let after = observe(state, desktop, visit).await?;
    let q = quiet(state, desktop, visit).await?;
    stage(
        state,
        desktop,
        name,
        "recovered",
        visit,
        json!({"before":before,"after":after,"quiet":q}),
        captures,
        stages,
    )
    .await?;
    Ok(())
}
