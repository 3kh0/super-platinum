//! Original inline confirmation, ordered responses, and retained account custody.
use super::*;
use crate::app_home::about::management::fixture::unpublish as endpoint;
use crate::app_home::about::management::unpublish as product;

async fn native(desktop: &DesktopContext, capture: bool) -> Result<Value, String> {
    wait_dom(
        desktop,
        &include_str!("unpublish/native.js")
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
        json!({"model":snapshot(&s),"unpublish":product::snapshot(&s,visit),"server":endpoint::snapshot(&s),"native":native}),
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
        "{label}: unpublish={} server={}",
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
            "Held Unpublish changed without delivery: before={before} after={after}"
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
        return Err("Unpublish synthetic isolation changed".into());
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
        return Err("Unpublish capture must be fresh".into());
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
            "Unpublish capture changed: {label}: before={observed} after={after}"
        ));
    }
    let mut geometry = observed["native"].clone();
    geometry["pngDimensions"] = json!([width, height]);
    captures.insert(label.into(), json!(path.to_string_lossy()));
    stages.push(json!({"stage":label,"model":observed["model"],"geometry":geometry,
        "proof":{"unpublish":observed["unpublish"],"server":observed["server"],"native":observed["native"],"exercise":exercise}}));
    eprintln!(
        "workflow-management-capture: {}",
        json!({"stage":label,"edge":"after"})
    );
    Ok(())
}
async fn checked(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
) -> Result<Value, String> {
    let before = observe(state, desktop, visit).await?;
    let reads = before["model"]["reads"]
        .as_u64()
        .ok_or("Read count missing")?;
    click(desktop, "workflow-management-check").await?;
    wait(state, "explicit Unpublish readback", |v| {
        v["busy"] == false && v["reads"].as_u64() == Some(reads + 11)
    })
    .await?;
    until(state, visit, "readback retained", |d, _| {
        !d["attempts"][0]["readback"].is_null()
    })
    .await?;
    wait_dom(
        desktop,
        r#"({ok:!document.getElementById('workflow-management')?.__workflowManagement?.lease})"#,
    )
    .await?;
    let after = observe(state, desktop, visit).await?;
    if !draft(&after) || before["model"]["writes"] != after["model"]["writes"] {
        return Err("Readback lost draft or replayed mutation".into());
    }
    Ok(json!({"before":before,"after":after}))
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
    let model = wait(state, "Unpublish Configuration destination", |v| {
        v["busy"] == false && v["route"] == "editor"
    })
    .await?;
    let mut visit = model["visit"]
        .as_u64()
        .ok_or("Original Unpublish visit missing")?;
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
    click(desktop, "workflow-management-unpublish").await?;
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
    if case == "unpublish-success" {
        let before = observe(state, desktop, visit).await?;
        click(desktop, "workflow-unpublish-cancel").await?;
        until(state, visit, "Cancel original confirmation", |d, _| {
            d["confirmation"].is_null()
        })
        .await?;
        wait_dom(desktop,r#"({ok:document.getElementById('workflow-management-title')?.readOnly===false&&!document.getElementById('workflow-unpublish-confirmation')})"#).await?;
        let after = observe(state, desktop, visit).await?;
        if !draft(&after)
            || before["server"] != after["server"]
            || before["unpublish"]["document"]["id"] != after["unpublish"]["document"]["id"]
            || after["unpublish"]["document"]["revision"].as_u64()
                <= before["unpublish"]["document"]["revision"].as_u64()
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
        click(desktop, "workflow-management-unpublish").await?;
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
    click(desktop, "workflow-unpublish-confirm").await?;
    if case == "unpublish-retirement" {
        until(state, visit, "original response held", |d, s| {
            !s["held"].is_null() && d["attempts"][0]["finished"] == false
        })
        .await?;
        let held = quiet(state, desktop, visit).await?;
        stage(
            state,
            desktop,
            name,
            "primary-held",
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
            || successor["unpublish"]["visible"] != false
            || successor["unpublish"]["attempts"][0]["current"] != false
        {
            return Err("Replacement retained original Unpublish authority".into());
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
        until(state, visit, "old Unpublish worker finished", |d, _| {
            d["attempts"][0]["finished"] == true
        })
        .await?;
        let after = observe(state, desktop, visit).await?;
        if successor["model"] != after["model"]
            || successor["native"] != after["native"]
            || after["unpublish"]["attempts"][0]["resolved"] != false
            || successor["server"]["requests"] != after["server"]["requests"]
            || successor["server"]["current"] != after["server"]["current"]
        {
            return Err(format!(
                "Late Unpublish changed successor or dispatched: before={successor} after={after}"
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
    } else if case == "unpublish-success" {
        until(
            state,
            visit,
            "first ACK and original second response",
            |d, s| {
                !s["held"].is_null()
                    && d["attempts"][0]["slots"][0]["outcome"] == "acknowledged"
                    && d["attempts"][0]["followup"]["stages"][0]["slots"][0]["outcome"]
                        == "in-flight"
            },
        )
        .await?;
        let held = quiet(state, desktop, visit).await?;
        stage(
            state,
            desktop,
            name,
            "primary-ack",
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
        until(state, visit, "complete Unpublish acknowledged", |d, _| {
            d["attempts"][0]["complete"] == true && d["attempts"][0]["finished"] == true
        })
        .await?;
        stage(
            state,
            desktop,
            name,
            "confirmed",
            visit,
            json!({"release":release}),
            &mut captures,
            &mut stages,
        )
        .await?;
        let exercise = checked(state, desktop, visit).await?;
        if exercise["after"]["unpublish"]["attempts"][0]["resolved"] != true
            || exercise["after"]["unpublish"]["document"]["editable"] != true
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
        let reopened = wait(state, "reopened unpublished workflow", |v| {
            v["busy"] == false && v["route"] == "editor"
        })
        .await?;
        visit = reopened["visit"].as_u64().ok_or("Reopened visit missing")?;
        // Reopen legitimately mounts new native nodes, while retaining the exact document/draft.
        let remount=wait_dom(desktop,r#"(()=>{const old=window.__workflowUnpublishOriginal,root=document.getElementById('workflow-management'),title=document.getElementById('workflow-management-title'),description=document.getElementById('workflow-management-description');return{ok:!!old&&!!root?.__workflowManagement&&!!title&&!!description&&!old.root.isConnected&&old.root!==root&&old.title!==title&&old.description!==description,originalDetached:!!old&&!old.root.isConnected,newRoot:!!old&&old.root!==root,newTitle:!!old&&old.title!==title,newDescription:!!old&&old.description!==description}})()"#).await?;
        native(desktop, true).await?;
        let after = observe(state, desktop, visit).await?;
        if !draft(&after)
            || after["unpublish"]["document"]["id"] != before["unpublish"]["document"]["id"]
            || after["unpublish"]["document"]["pending"] != Value::Null
            || after["unpublish"]["attempts"] != before["unpublish"]["attempts"]
        {
            return Err(format!(
                "Reopen lost original publication or draft: before={before} after={after}"
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
        until(state, visit, "retained Unpublish refusal", |d, _| {
            d["attempts"][0]["finished"] == true
        })
        .await?;
        let label = match case.as_str() {
            "unpublish-primary-rejected" => "primary-rejected",
            "unpublish-primary-unknown" => "primary-unconfirmed",
            "unpublish-sync-rejected" => "sync-rejected",
            _ => "sync-unconfirmed",
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
        json!({"ok":true,"fixture":name,"scenario":"editor","unpublishCase":case,"readiness":readiness,
        "syntheticSession":true,"noLiveNetwork":true,"aboutConfiguration":true,"viewport":stages[0]["geometry"]["viewport"],
        "stages":stages,"captures":captures,"final":final_state,"replacement":replacement}),
    )
}
