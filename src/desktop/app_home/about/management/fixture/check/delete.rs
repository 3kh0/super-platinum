//! Original inline confirmation, ordered responses, and retained account custody.
use super::*;
use crate::app_home::about::management::delete as product;
use crate::app_home::about::management::fixture::delete as endpoint;

async fn native(desktop: &DesktopContext, capture: bool) -> Result<Value, String> {
    wait_dom(
        desktop,
        &include_str!("delete/native.js")
            .replace("CAPTURE", if capture { "true" } else { "false" }),
    )
    .await
}
async fn observe(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
) -> Result<Value, String> {
    let native = native(desktop, false).await?;
    let s = state.peek();
    Ok(
        json!({"model":snapshot(&s),"delete":product::snapshot(&s,visit),"server":endpoint::snapshot(&s),"native":native}),
    )
}
async fn until(
    state: Signal<ShellState>,
    visit: u64,
    label: &str,
    accept: impl Fn(&Value, &Value) -> bool,
) -> Result<(), String> {
    for _ in 0..200 {
        {
            let s = state.peek();
            if accept(&product::snapshot(&s, visit), &endpoint::snapshot(&s)) {
                return Ok(());
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!(
        "{label}: delete={} server={}",
        product::snapshot(&state.peek(), visit),
        endpoint::snapshot(&state.peek())
    ))
}
async fn quiet(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
) -> Result<Value, String> {
    let before = observe(state, desktop, visit).await?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let after = observe(state, desktop, visit).await?;
    if before != after {
        return Err(format!(
            "Held Delete changed without delivery: before={before} after={after}"
        ));
    }
    Ok(json!({"before":before,"after":after,"observedMs":500}))
}
async fn stage(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    label: &str,
    visit: u64,
    exercise: Value,
    captures: &mut serde_json::Map<String, Value>,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    if !isolated(&state.peek()) {
        return Err("Delete synthetic isolation changed".into());
    }
    let observed = observe(state, desktop, visit).await?;
    let directory = std::env::var_os("SUPER_PLATINUM_UI_CAPTURE_DIR")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or("Absolute capture directory required")?;
    let path = directory.join(format!("{name}-{label}.png"));
    let inspect = path.clone();
    if tokio::task::spawn_blocking(move || inspect.try_exists())
        .await
        .map_err(|_| "Inspect capture worker failed")?
        .map_err(|_| "Inspect capture path failed")?
    {
        return Err("Delete capture must be fresh".into());
    }
    eprintln!(
        "workflow-management-capture: {}",
        json!({"stage":label,"edge":"before"})
    );
    let (path, width, height) =
        crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), desktop).await?;
    let after = observe(state, desktop, visit).await?;
    if observed != after {
        return Err(format!(
            "Delete capture changed: {label}: before={observed} after={after}"
        ));
    }
    let mut geometry = observed["native"].clone();
    geometry["pngDimensions"] = json!([width, height]);
    captures.insert(label.into(), json!(path.to_string_lossy()));
    stages.push(json!({"stage":label,"model":observed["model"],"geometry":geometry,
        "proof":{"delete":observed["delete"],"server":observed["server"],"native":observed["native"],"exercise":exercise}}));
    eprintln!(
        "workflow-management-capture: {}",
        json!({"stage":label,"edge":"after"})
    );
    Ok(())
}
async fn disabled_check(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
) -> Result<Value, String> {
    let before = observe(state, desktop, visit).await?;
    let activation=wait_dom(desktop,r#"(()=>{const e=document.getElementById('workflow-management-check');if(!e||!e.isConnected||!e.disabled)return{ok:false};e.click();return{ok:true,id:e.id,disabled:e.disabled,sameNode:e===document.getElementById(e.id)}})()"#).await?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let after = observe(state, desktop, visit).await?;
    if before != after {
        return Err(format!(
            "Disabled original Check replayed or changed Delete: {before} -> {after}"
        ));
    }
    Ok(json!({"activation":activation,"before":before,"after":after,"observedMs":500}))
}
fn draft(v: &Value) -> bool {
    v["model"]["title"] == TITLE
        && v["model"]["description"] == DESCRIPTION
        && v["native"]["originalNodes"] == true
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
    let model = wait(state, "Delete Configuration destination", |v| {
        v["busy"] == false && v["route"] == "editor"
    })
    .await?;
    let visit = model["visit"]
        .as_u64()
        .ok_or("Original Delete visit missing")?;
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
    click(desktop, "workflow-management-delete").await?;
    until(state, visit, "original confirmation", |d, _| {
        d["confirmation"]["current"] == true
    })
    .await?;
    stage(
        state,
        desktop,
        name,
        "confirmation",
        visit,
        json!({}),
        &mut captures,
        &mut stages,
    )
    .await?;
    if case == "delete-success" {
        let before = observe(state, desktop, visit).await?;
        click(desktop, "workflow-delete-cancel").await?;
        until(state, visit, "Cancel original confirmation", |d, _| {
            d["confirmation"].is_null()
        })
        .await?;
        wait_dom(desktop,r#"({ok:document.getElementById('workflow-management-title')?.readOnly===false&&!document.getElementById('workflow-delete-confirmation')})"#).await?;
        let after = observe(state, desktop, visit).await?;
        if !draft(&after)
            || before["server"] != after["server"]
            || before["delete"]["document"]["id"] != after["delete"]["document"]["id"]
            || after["delete"]["document"]["revision"].as_u64()
                <= before["delete"]["document"]["revision"].as_u64()
        {
            return Err("Cancel changed draft, source, or original document custody".into());
        }
        stage(
            state,
            desktop,
            name,
            "cancelled",
            visit,
            json!({"before":before,"after":after}),
            &mut captures,
            &mut stages,
        )
        .await?;
        click(desktop, "workflow-management-delete").await?;
        until(state, visit, "second original confirmation", |d, _| {
            d["confirmation"]["current"] == true
        })
        .await?;
        stage(
            state,
            desktop,
            name,
            "reconfirmed",
            visit,
            json!({}),
            &mut captures,
            &mut stages,
        )
        .await?;
    }
    endpoint::arm(&state.peek())?;
    let original = endpoint::capture(&state.peek())?;
    click(desktop, "workflow-delete-confirm").await?;
    if case.ends_with("retirement") {
        until(state, visit, "original response held", |d, s| {
            !s["held"].is_null() && d["attempts"][0]["finished"] == false
        })
        .await?;
        let held = quiet(state, desktop, visit).await?;
        stage(
            state,
            desktop,
            name,
            if case == "delete-preflight-retirement" {
                "preflight-held"
            } else {
                "mutation-held"
            },
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
            || successor["delete"]["visible"] != false
            || successor["delete"]["attempts"][0]["current"] != false
        {
            return Err("Replacement retained original Delete authority".into());
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
        until(state, visit, "old Delete worker finished", |d, _| {
            d["attempts"][0]["finished"] == true
        })
        .await?;
        let after = observe(state, desktop, visit).await?;
        if successor["model"] != after["model"]
            || successor["native"] != after["native"]
            || after["delete"]["terminal"] != false
            || after["delete"]["attempts"][0]["terminal"] != false
            || successor["server"]["requests"] != after["server"]["requests"]
            || successor["server"]["triggerPresent"] != after["server"]["triggerPresent"]
            || successor["server"]["workflowPresent"] != after["server"]["workflowPresent"]
        {
            return Err(format!(
                "Late Delete changed successor or dispatched: before={successor} after={after}"
            ));
        }
        stage(
            state,
            desktop,
            name,
            if case == "delete-preflight-retirement" {
                "late-refused"
            } else {
                "late-settled"
            },
            visit,
            json!({"release":release,"before":successor,"after":after,"successorUnchanged":true}),
            &mut captures,
            &mut stages,
        )
        .await?;
    } else if case == "delete-success" {
        until(
            state,
            visit,
            "first ACK and original second response",
            |d, s| {
                !s["held"].is_null()
                    && d["attempts"][0]["slots"][0]["outcome"] == "acknowledged"
                    && d["attempts"][0]["slots"][1]["outcome"] == "in-flight"
            },
        )
        .await?;
        let held = quiet(state, desktop, visit).await?;
        stage(
            state,
            desktop,
            name,
            "trigger-ack",
            visit,
            held,
            &mut captures,
            &mut stages,
        )
        .await?;
        let release = original.release()?;
        if release["delivered"] != true || release["receiverRetired"] != false {
            return Err("Original second ACK receiver changed".into());
        }
        until(state, visit, "complete Delete acknowledged", |d, _| {
            d["terminal"] == true && d["attempts"][0]["finished"] == true
        })
        .await?;
        stage(
            state,
            desktop,
            name,
            "deleted",
            visit,
            json!({"release":release}),
            &mut captures,
            &mut stages,
        )
        .await?;
        click(desktop, "workflow-management-managed").await?;
        wait(state, "actual empty managed refresh", |v| {
            v["route"] == "managed" && v["busy"] == false
        })
        .await?;
        wait_dom(desktop,r#"({ok:!document.getElementById('workflow-management-open-WF_REVIEW')&&document.getElementById('workflow-management')?.textContent.includes('No matching workflows.')})"#).await?;
        stage(
            state,
            desktop,
            name,
            "managed-empty",
            visit,
            json!({}),
            &mut captures,
            &mut stages,
        )
        .await?;
    } else {
        until(state, visit, "retained Delete refusal", |d, _| {
            d["attempts"][0]["finished"] == true
        })
        .await?;
        let label = match case.as_str() {
            "delete-first-rejected" => "rejected",
            "delete-first-unknown" => "unconfirmed",
            _ => "partial-unconfirmed",
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
            let exercise = disabled_check(state, desktop, visit).await?;
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
        json!({"ok":true,"fixture":name,"scenario":"editor","deleteCase":case,"readiness":readiness,
        "syntheticSession":true,"noLiveNetwork":true,"aboutConfiguration":true,"viewport":stages[0]["geometry"]["viewport"],
        "stages":stages,"captures":captures,"final":final_state,"replacement":replacement}),
    )
}
