//! Copy-access controls through the original workflow editor and exact raw receipts.
use super::*;
use crate::app_home::about::management::fixture::permissions as endpoint;
use crate::app_home::about::management::permissions as product;
mod evidence;
use evidence::{checked, draft, native, observe, quiet, stage, until};

async fn open(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
) -> Result<(), String> {
    click(desktop, "workflow-management-permissions").await?;
    until(state, visit, "original Permissions controls", |d, _| {
        d["confirmation"]["current"] == true
    })
    .await?;
    native(desktop, false).await?;
    Ok(())
}
async fn choose(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
) -> Result<(), String> {
    wait_dom(desktop,r#"(()=>{const e=document.getElementById('workflow-permissions-copy');if(!e||e.disabled)return{ok:false};e.scrollIntoView({block:'center'});e.focus();const r=e.getBoundingClientRect(),h=document.elementFromPoint(r.x+r.width/2,r.y+r.height/2);if(h!==e&&!e.contains(h))return{ok:false};Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(e,'everyone');e.dispatchEvent(new Event('change',{bubbles:true}));return{ok:true};})()"#).await?;
    until(state, visit, "original selected copy access", |d, _| {
        d["confirmation"]["selected"] == "everyone"
    })
    .await?;
    Ok(())
}
async fn close_local(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
    id: &str,
) -> Result<Value, String> {
    let before = observe(state, desktop, visit).await?;
    click(desktop, id).await?;
    until(state, visit, "closed local settings", |d, _| {
        d["confirmation"].is_null()
    })
    .await?;
    wait_dom(desktop,r#"({ok:document.getElementById('workflow-management-title')?.readOnly===false&&!document.getElementById('workflow-permissions')})"#).await?;
    let after = observe(state, desktop, visit).await?;
    if !draft(&after)
        || before["server"] != after["server"]
        || before["permissions"]["document"]["id"] != after["permissions"]["document"]["id"]
        || after["permissions"]["document"]["revision"].as_u64()
            <= before["permissions"]["document"]["revision"].as_u64()
    {
        return Err(
            "No-op/Cancel changed source, native draft, or original document custody".into(),
        );
    }
    Ok(json!({"action":id,"before":before,"after":after}))
}

pub(super) async fn run(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    let readiness = readiness(name).await?;
    let case = endpoint::case();
    let mut captures = serde_json::Map::new();
    let mut stages = Vec::new();
    let mut replacement = Value::Null;
    wait_dom(desktop,r#"({ok:!!document.getElementById('workflow-configuration')&&!document.getElementById('workflow-management')})"#).await?;
    stage(
        state,
        desktop,
        name,
        "about",
        0,
        json!({}),
        &mut captures,
        &mut stages,
    )
    .await?;
    click(desktop, "workflow-configuration").await?;
    let model = wait(state, "Permissions Configuration destination", |v| {
        v["busy"] == false && v["route"] == "editor"
    })
    .await?;
    let mut visit = model["visit"]
        .as_u64()
        .ok_or("Original Permissions visit missing")?;
    native(desktop, true).await?;
    stage(
        state,
        desktop,
        name,
        "editor",
        visit,
        json!({}),
        &mut captures,
        &mut stages,
    )
    .await?;
    edit(state, desktop, TITLE).await?;
    stage(
        state,
        desktop,
        name,
        "draft",
        visit,
        json!({}),
        &mut captures,
        &mut stages,
    )
    .await?;
    open(state, desktop, visit).await?;
    if case == "permissions-success" {
        stage(
            state,
            desktop,
            name,
            "permissions",
            visit,
            json!({}),
            &mut captures,
            &mut stages,
        )
        .await?;
        let exercise = close_local(state, desktop, visit, "workflow-permissions-save").await?;
        stage(
            state,
            desktop,
            name,
            "unchanged",
            visit,
            exercise,
            &mut captures,
            &mut stages,
        )
        .await?;
        open(state, desktop, visit).await?;
    }
    choose(state, desktop, visit).await?;
    stage(
        state,
        desktop,
        name,
        "selected",
        visit,
        json!({}),
        &mut captures,
        &mut stages,
    )
    .await?;
    if case == "permissions-success" {
        let exercise = close_local(state, desktop, visit, "workflow-permissions-cancel").await?;
        stage(
            state,
            desktop,
            name,
            "cancelled",
            visit,
            exercise,
            &mut captures,
            &mut stages,
        )
        .await?;
        open(state, desktop, visit).await?;
        choose(state, desktop, visit).await?;
    }
    let admission = observe(state, desktop, visit).await?;
    endpoint::arm(&state.peek())?;
    let original = endpoint::capture(&state.peek())?;
    click(desktop, "workflow-permissions-save").await?;
    if case == "permissions-retirement" {
        until(state, visit, "original response held", |d, s| {
            !s["held"].is_null() && d["attempts"][0]["finished"] == false
        })
        .await?;
        let held = quiet(state, desktop, visit).await?;
        stage(
            state,
            desktop,
            name,
            "held",
            visit,
            held,
            &mut captures,
            &mut stages,
        )
        .await?;
        let before = observe(state, desktop, visit).await?;
        replacement = replace_account(state)?;
        wait_dom(desktop,r#"({ok:!!document.getElementById('workflow-configuration')&&!document.getElementById('workflow-management')})"#).await?;
        let successor = observe(state, desktop, visit).await?;
        if successor["server"]["ownerCurrent"] != false
            || successor["permissions"]["visible"] != false
            || successor["permissions"]["attempts"][0]["current"] != false
        {
            return Err("Replacement retained original Permissions authority".into());
        }
        stage(
            state,
            desktop,
            name,
            "successor",
            visit,
            json!({"replacement":replacement,"before":before,"after":successor}),
            &mut captures,
            &mut stages,
        )
        .await?;
        let release = original.release()?;
        if release["delivered"] != true || release["receiverRetired"] != false {
            return Err(format!("Original response receiver changed: {release}"));
        }
        until(state, visit, "old Permissions worker finished", |d, _| {
            d["attempts"][0]["finished"] == true
        })
        .await?;
        let after = observe(state, desktop, visit).await?;
        if successor["model"] != after["model"]
            || successor["native"] != after["native"]
            || after["permissions"]["attempts"][0]["resolved"] != false
            || successor["server"]["requests"] != after["server"]["requests"]
            || successor["server"]["current"] != after["server"]["current"]
        {
            return Err(format!(
                "Late Permissions changed successor or dispatched: before={successor} after={after}"
            ));
        }
        stage(
            state,
            desktop,
            name,
            "late-settled",
            visit,
            json!({"release":release,"before":successor,"after":after,"successorUnchanged":true}),
            &mut captures,
            &mut stages,
        )
        .await?;
    } else if case == "permissions-success" {
        until(state, visit, "copy permission acknowledged", |d, _| {
            d["attempts"][0]["complete"] == true && d["attempts"][0]["finished"] == true
        })
        .await?;
        stage(
            state,
            desktop,
            name,
            "acknowledged",
            visit,
            json!({"admission":admission}),
            &mut captures,
            &mut stages,
        )
        .await?;
        let exercise = checked(state, desktop, visit).await?;
        if exercise["after"]["permissions"]["attempts"][0]["resolved"] != true
            || exercise["after"]["permissions"]["document"]["editable"] != true
        {
            return Err("Complete readback did not restore original draft editing".into());
        }
        stage(
            state,
            desktop,
            name,
            "checked",
            visit,
            exercise,
            &mut captures,
            &mut stages,
        )
        .await?;
        let before = observe(state, desktop, visit).await?;
        click(desktop, "workflow-management-back").await?;
        wait_dom(desktop,r#"({ok:!!document.getElementById('workflow-configuration')&&!document.getElementById('workflow-management')})"#).await?;
        let after = observe(state, desktop, visit).await?;
        if before["server"] != after["server"] {
            return Err("Back dispatched a request".into());
        }
        stage(
            state,
            desktop,
            name,
            "back",
            visit,
            json!({"before":before,"after":after}),
            &mut captures,
            &mut stages,
        )
        .await?;
        click(desktop, "workflow-configuration").await?;
        let reopened = wait(state, "reopened workflow permissions", |v| {
            v["busy"] == false && v["route"] == "editor"
        })
        .await?;
        visit = reopened["visit"].as_u64().ok_or("Reopened visit missing")?;
        // Reopen legitimately mounts new native nodes, while retaining the exact document/draft.
        let remount=wait_dom(desktop,r#"(()=>{const old=window.__workflowPermissionsOriginal,root=document.getElementById('workflow-management'),title=document.getElementById('workflow-management-title'),description=document.getElementById('workflow-management-description');return{ok:!!old&&!!root?.__workflowManagement&&!!title&&!!description&&!old.root.isConnected&&old.root!==root&&old.title!==title&&old.description!==description,originalDetached:!!old&&!old.root.isConnected,newRoot:!!old&&old.root!==root,newTitle:!!old&&old.title!==title,newDescription:!!old&&old.description!==description}})()"#).await?;
        native(desktop, true).await?;
        let after = observe(state, desktop, visit).await?;
        if !draft(&after)
            || after["permissions"]["document"]["id"] != before["permissions"]["document"]["id"]
            || after["permissions"]["document"]["pending"] != Value::Null
            || after["permissions"]["attempts"] != before["permissions"]["attempts"]
        {
            return Err(format!(
                "Reopen lost original permissions or draft: before={before} after={after}"
            ));
        }
        stage(
            state,
            desktop,
            name,
            "reopened",
            visit,
            json!({"before":before,"after":after,"remount":remount}),
            &mut captures,
            &mut stages,
        )
        .await?;
    } else {
        until(state, visit, "retained Permissions refusal", |d, _| {
            d["attempts"][0]["finished"] == true
        })
        .await?;
        let label = match case.as_str() {
            "permissions-rejected" => "rejected",
            _ => "unconfirmed",
        };
        stage(
            state,
            desktop,
            name,
            label,
            visit,
            json!({}),
            &mut captures,
            &mut stages,
        )
        .await?;
        for label in ["checked", "checked-again"] {
            let exercise = checked(state, desktop, visit).await?;
            stage(
                state,
                desktop,
                name,
                label,
                visit,
                exercise,
                &mut captures,
                &mut stages,
            )
            .await?;
        }
    }
    let final_state = observe(state, desktop, visit).await?;
    Ok(
        json!({"ok":true,"fixture":name,"scenario":"editor","permissionsCase":case,"readiness":readiness,
        "syntheticSession":true,"noLiveNetwork":true,"aboutConfiguration":true,"viewport":stages[0]["geometry"]["viewport"],
        "stages":stages,"captures":captures,"final":final_state,"replacement":replacement}),
    )
}
