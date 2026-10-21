//! Actual post-Add tab browsing and real About reentry. No product invocation.
use super::*;
use crate::channel_workflows::fixture as server;
mod rate;

pub(super) async fn settled(desktop: &DesktopContext, status: &str) -> Result<(), String> {
    wait_dom(desktop,&format!(r#"(()=>{{const e=document.getElementById('channel-workflows-panel');let p;try{{p=JSON.parse(e?.dataset.fixtureReadback)}}catch{{return{{ok:false}}}}return{{ok:p.current===true&&p.status==={},product:p}}}})()"#,json!(status))).await.map(|_|())
}
pub(super) async fn tab(desktop: &DesktopContext, label: &str) -> Result<(), String> {
    wait_dom(desktop,&format!(r#"(()=>{{const e=[...document.querySelectorAll('.channel-tabs button')].find(e=>e.textContent.trim()==={});if(!e||e.disabled)return{{ok:false}};e.scrollIntoView({{block:'center',inline:'center'}});const r=e.getBoundingClientRect(),h=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);if(h!==e&&!e.contains(h))return{{ok:false}};e.click();return{{ok:true}}}})()"#,json!(label))).await.map(|_|())
}
pub(super) async fn sidebar(desktop: &DesktopContext, label: &str) -> Result<(), String> {
    wait_dom(desktop,&format!(r#"(()=>{{const e=[...document.querySelectorAll('.sidebar button.channel')].find(e=>e.querySelector('.channel-name')?.textContent==={});if(!e)return{{ok:false}};e.scrollIntoView({{block:'center'}});const r=e.getBoundingClientRect(),h=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);if(h!==e&&!e.contains(h))return{{ok:false}};e.click();return{{ok:true}}}})()"#,json!(label))).await.map(|_|())
}
pub(super) async fn return_document(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
) -> Result<Value, String> {
    let before = observe(state, desktop, visit).await?;
    sidebar(desktop, "Review requests").await?;
    let mut selected = false;
    for _ in 0..100 {
        if state.peek().core.active_channel.as_deref() == Some("CAPPHOME1") {
            selected = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    if !selected {
        return Err("Original app DM was not selected".into());
    }
    let scope = state.peek().timeline_scope();
    evidence::observed_wait(state,desktop,visit,"original app DM before About",&format!(r#"(()=>{{const h=document.querySelector('.conversation-header .header-name'),t=document.getElementById('message-timeline'),s=document.querySelector('.sidebar button.channel.active .channel-name');return{{ok:h?.textContent==='Review requests'&&s?.textContent==='Review requests'&&t?.dataset.timelineScope==={},header:h?.textContent,selected:s?.textContent,timelineScope:t?.dataset.timelineScope}}}})()"#,json!(scope))).await?;
    tab(desktop, "About").await?;
    evidence::observed_wait(state,desktop,visit,"fresh About Configuration",r#"(()=>{const c=document.getElementById('workflow-configuration');return{ok:!!c&&!c.disabled,configurationPresent:!!c,disabled:c?.disabled}})()"#).await?;
    click(desktop, "workflow-configuration").await?;
    let current = wait(
        state,
        "real About reattached original dirty document",
        |m| {
            m["route"] == "editor"
                && m["busy"] == false
                && m["title"] == TITLE
                && m["description"] == DESCRIPTION
        },
    )
    .await?;
    let rebound=wait_dom(desktop,r#"(()=>{const r=window.__firstPublishRegistry,root=document.getElementById('workflow-management'),title=document.getElementById('workflow-management-title'),description=document.getElementById('workflow-management-description');return{ok:!!r?.parent&&!r.parent.root.isConnected&&!!root&&!!title&&!!description&&root!==r.parent.root&&title!==r.parent.title&&description!==r.parent.description,originalDetached:!!r?.parent&&!r.parent.root.isConnected,newRoot:!!root&&root!==r?.parent?.root,newTitle:!!title&&title!==r?.parent?.title,newDescription:!!description&&description!==r?.parent?.description}})()"#).await?;
    native(desktop, "parent").await?;
    let after = observe(state, desktop, visit).await?;
    if before["model"]["confirmationDestination"]["documents"]
        != after["model"]["confirmationDestination"]["documents"]
        || current["visit"] == json!(visit)
    {
        return Err(format!(
            "Workflows return changed original draft: before={before} after={after}"
        ));
    }
    Ok(json!({"before":before,"after":after,"remount":rebound,"freshVisit":current["visit"]}))
}
pub(super) async fn run(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    visit: u64,
    admission: Value,
    captures: &mut serde_json::Map<String, Value>,
    stages: &mut Vec<Value>,
) -> Result<Value, String> {
    let case = endpoint::case();
    if case == "first-tab-rate-limit" {
        rate::run(state, desktop, name, visit, admission, captures, stages).await?;
    } else if case == "first-tab-retirement" {
        wait_dom(desktop,r#"(()=>{let p;try{p=JSON.parse(document.getElementById('channel-workflows-panel')?.dataset.fixtureReadback)}catch{return{ok:false}}return{ok:p.current===true&&p.status==='loading'&&p.phase==='previews'}})()"#).await?;
        let q = quiet(state, desktop, visit).await?;
        stage(
            state,
            desktop,
            name,
            "workflows-held",
            visit,
            json!({"admission":admission,"quiet":q}),
            captures,
            stages,
        )
        .await?;
        let model =
            server::capture(&state.peek()).map_err(|_| "Original Workflows server absent")?;
        let serial = model.snapshot()["held"][0]["serial"]
            .as_u64()
            .ok_or("Original preview sender absent")? as usize;
        let before = observe(state, desktop, visit).await?;
        tab(desktop, "Messages").await?;
        wait_dom(desktop,r#"({ok:!document.getElementById('channel-workflows-panel')&&!!document.getElementById('message-timeline')&&!!document.getElementById('channel-composer')})"#).await?;
        let mut retired = false;
        for _ in 0..100 {
            if model.snapshot()["held"][0]["receiverRetired"] == true {
                retired = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        if !retired {
            return Err(format!(
                "Original tab receiver survived unmount: {}",
                model.snapshot()
            ));
        }
        let successor = observe(state, desktop, visit).await?;
        stage(
            state,
            desktop,
            name,
            "switched",
            visit,
            json!({"before":before,"after":successor}),
            captures,
            stages,
        )
        .await?;
        let release = model.release(serial)?;
        if release["delivered"] != false || release["receiverRetired"] != true {
            return Err("Retired original preview delivery changed".into());
        }
        let after = observe(state, desktop, visit).await?;
        for key in ["model", "firstPublish", "native"] {
            if successor[key] != after[key] {
                return Err(format!("Late preview changed successor {key}"));
            }
        }
        let q = quiet(state, desktop, visit).await?;
        stage(
            state,
            desktop,
            name,
            "late-settled",
            visit,
            json!({"before":successor,"release":release,"after":after,"quiet":q}),
            captures,
            stages,
        )
        .await?;
    } else {
        let failure = case == "first-tab-failure";
        settled(desktop, if failure { "failed" } else { "ready" }).await?;
        let q = quiet(state, desktop, visit).await?;
        stage(
            state,
            desktop,
            name,
            if failure {
                "workflows-failed"
            } else {
                "workflows"
            },
            visit,
            json!({"admission":admission,"quiet":q}),
            captures,
            stages,
        )
        .await?;
        if case == "first-tab-empty" {
            let before = observe(state, desktop, visit).await?;
            sidebar(desktop, "private-review").await?;
            wait_dom(desktop,r#"({ok:document.getElementById('channel-details-trigger')?.textContent==='#private-review'})"#).await?;
            tab(desktop, "Workflows").await?;
            settled(desktop, "ready").await?;
            let after = observe(state, desktop, visit).await?;
            stage(
                state,
                desktop,
                name,
                "empty",
                visit,
                json!({"before":before,"after":after}),
                captures,
                stages,
            )
            .await?;
        }
        let before = observe(state, desktop, visit).await?;
        let serial = before["native"]["workflows"]["product"]["serial"]
            .as_u64()
            .ok_or("Actual tab read serial absent")?;
        click(
            desktop,
            if failure {
                "channel-workflows-retry"
            } else {
                "channel-workflows-refresh"
            },
        )
        .await?;
        wait_dom(desktop,&format!(r#"(()=>{{let p;try{{p=JSON.parse(document.getElementById('channel-workflows-panel')?.dataset.fixtureReadback)}}catch{{return{{ok:false}}}}return{{ok:p.current===true&&p.status==='ready'&&p.serial>{serial}}}}})()"#)).await?;
        let after = observe(state, desktop, visit).await?;
        let q = quiet(state, desktop, visit).await?;
        stage(
            state,
            desktop,
            name,
            if failure { "recovered" } else { "refreshed" },
            visit,
            json!({"before":before,"after":after,"quiet":q}),
            captures,
            stages,
        )
        .await?;
    }
    let exercise = return_document(state, desktop, visit).await?;
    stage(
        state, desktop, name, "returned", visit, exercise, captures, stages,
    )
    .await?;
    Ok(Value::Null)
}
