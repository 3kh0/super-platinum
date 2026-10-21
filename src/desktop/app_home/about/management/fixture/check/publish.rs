//! Native clean-draft admission, acquired validation, and original Publish receipts.
use super::*;
use crate::app_home::about::management::fixture::publish as endpoint;
use crate::app_home::about::management::publish as product;

async fn native(desktop: &DesktopContext, capture: bool) -> Result<Value, String> {
    let case = endpoint::case();
    let caption = if case.starts_with("publish-republish-") && case != "publish-republish-changed" {
        "Publish"
    } else {
        "Publish Changes"
    };
    wait_dom(
        desktop,
        &include_str!("publish/native.js")
            .replace("CAPTURE", if capture { "true" } else { "false" })
            .replace("EXPECTED_PUBLISH_CAPTION", &json!(caption).to_string()),
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
        json!({"model":snapshot(&s),"publish":product::snapshot(&s,visit),"server":endpoint::snapshot(&s),"native":native}),
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
        "{label}: publish={} server={}",
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
            "Held Publish changed without delivery: before={before} after={after}"
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
        return Err("Publish synthetic isolation changed".into());
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
        return Err("Publish capture must be fresh".into());
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
            "Publish capture changed: {label}: before={observed} after={after}"
        ));
    }
    let mut geometry = observed["native"].clone();
    geometry["pngDimensions"] = json!([width, height]);
    captures.insert(label.into(), json!(path.to_string_lossy()));
    stages.push(json!({"stage":label,"model":observed["model"],"geometry":geometry,
        "proof":{"publish":observed["publish"],"server":observed["server"],"native":observed["native"],"exercise":exercise}}));
    eprintln!(
        "workflow-management-capture: {}",
        json!({"stage":label,"edge":"after"})
    );
    Ok(())
}

fn last(value: &Value) -> &Value {
    value["attempts"]
        .as_array()
        .and_then(|v| v.last())
        .unwrap_or(&Value::Null)
}
fn original_fields(value: &Value) -> bool {
    value["model"]["title"] == value["server"]["original"]["workflow"]["title"]
        && value["model"]["description"] == value["server"]["original"]["workflow"]["description"]
        && value["native"]["originalNodes"] == true
}
async fn checked(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
) -> Result<Value, String> {
    let before = observe(state, desktop, visit).await?;
    let reads = before["model"]["reads"]
        .as_u64()
        .ok_or("Original read count missing")?;
    click(desktop, "workflow-publish-check").await?;
    wait(state, "explicit Publish readback", |v| {
        v["busy"] == false && v["reads"].as_u64() == Some(reads + 11)
    })
    .await?;
    until(state, visit, "Publish readback retained", |p, _| {
        !last(p)["readback"].is_null()
    })
    .await?;
    wait_dom(
        desktop,
        r#"({ok:!document.getElementById('workflow-management')?.__workflowManagement?.lease})"#,
    )
    .await?;
    let after = observe(state, desktop, visit).await?;
    if !original_fields(&after) || before["model"]["writes"] != after["model"]["writes"] {
        return Err(format!(
            "Readback changed original fields or replayed mutation: before={before} after={after}"
        ));
    }
    Ok(json!({"before":before,"after":after}))
}

pub(super) async fn run(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    let readiness = readiness(name).await?;
    let case = endpoint::case();
    let scenario = endpoint::scenario();
    let mut captures = serde_json::Map::new();
    let mut stages = Vec::new();
    let mut replacement = Value::Null;
    wait_dom(desktop, r#"({ok:!!document.getElementById('workflow-configuration')&&!document.getElementById('workflow-management')})"#).await?;
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
    let model = wait(state, "Publish Configuration destination", |v| {
        v["busy"] == false && v["route"] == "editor"
    })
    .await?;
    let mut visit = model["visit"]
        .as_u64()
        .ok_or("Original Publish visit missing")?;
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
    let before = observe(state, desktop, visit).await?;
    let activation=wait_dom(desktop,r#"(()=>{const e=document.getElementById('workflow-management-publish'),root=document.getElementById('workflow-management');if(!e||!e.disabled||!root?.textContent.includes('Save details before publishing.'))return{ok:false};e.scrollIntoView({block:'center',behavior:'instant'});const r=e.getBoundingClientRect(),h=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);if(h!==e&&!e.contains(h))return{ok:false};e.click();return{ok:true,id:e.id,text:e.textContent,disabled:e.disabled,notice:'Save details before publishing.',hit:true};})()"#).await?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let after = observe(state, desktop, visit).await?;
    if before != after {
        return Err(format!(
            "Disabled dirty Publish changed source/native state: before={before} after={after}"
        ));
    }
    stage(
        state,
        desktop,
        name,
        "dirty-refused",
        visit,
        json!({"before":before,"after":after,"activation":activation,"observedMs":500}),
        &mut captures,
        &mut stages,
    )
    .await?;
    let title = after["server"]["original"]["workflow"]["title"]
        .as_str()
        .ok_or("Original title missing")?;
    let description = after["server"]["original"]["workflow"]["description"]
        .as_str()
        .ok_or("Original description missing")?;
    input(desktop, "workflow-management-title", title).await?;
    input(desktop, "workflow-management-description", description).await?;
    wait(state, "native original metadata restored", |v| {
        v["dirty"] == false && v["title"] == title && v["description"] == description
    })
    .await?;
    let restored = observe(state, desktop, visit).await?;
    if !original_fields(&restored) || restored["server"] != after["server"] {
        return Err("Restoring native metadata changed server or original nodes".into());
    }
    stage(
        state,
        desktop,
        name,
        "restored",
        visit,
        json!({"before":after,"after":restored}),
        &mut captures,
        &mut stages,
    )
    .await?;
    endpoint::arm(&state.peek())?;
    let original = endpoint::capture(&state.peek())?;
    click(desktop, "workflow-management-publish").await?;

    if scenario == "publish-validation-recovery" {
        until(state, visit, "original validation failed", |p, _| {
            last(p)["finished"] == true && last(p)["noSend"] == true
        })
        .await?;
        stage(
            state,
            desktop,
            name,
            "validation-failed",
            visit,
            json!({}),
            &mut captures,
            &mut stages,
        )
        .await?;
        let before = observe(state, desktop, visit).await?;
        click(desktop, "workflow-publish-retry").await?;
        until(
            state,
            visit,
            "original validation retry response held",
            |p, s| !s["held"].is_null() && last(p)["finished"] == false,
        )
        .await?;
        let held = quiet(state, desktop, visit).await?;
        stage(
            state,
            desktop,
            name,
            "retry-held",
            visit,
            json!({"before":before,"quiet":held}),
            &mut captures,
            &mut stages,
        )
        .await?;
        let release = original.release()?;
        if release["delivered"] != true || release["receiverRetired"] != false {
            return Err(format!("Original validation receiver retired: {release}"));
        }
        until(state, visit, "read-only validation ready", |p, _| {
            last(p)["finished"] == true
                && last(p)["noSend"] == true
                && last(p)["validation"]["status"] == "ready"
        })
        .await?;
        let after = observe(state, desktop, visit).await?;
        if before["model"]["writes"] != after["model"]["writes"] || !original_fields(&after) {
            return Err("Validation Retry mutated workflow or original native fields".into());
        }
        stage(
            state,
            desktop,
            name,
            "validation-ready",
            visit,
            json!({"release":release,"before":before,"after":after}),
            &mut captures,
            &mut stages,
        )
        .await?;
        click(desktop, "workflow-management-publish").await?;
    }
    if scenario == "publish-blocked" {
        until(
            state,
            visit,
            "actual publication validation blocked",
            |p, _| last(p)["finished"] == true && last(p)["noSend"] == true,
        )
        .await?;
        stage(
            state,
            desktop,
            name,
            "blocked",
            visit,
            json!({}),
            &mut captures,
            &mut stages,
        )
        .await?;
        let exercise = quiet(state, desktop, visit).await?;
        stage(
            state,
            desktop,
            name,
            "quiet",
            visit,
            exercise,
            &mut captures,
            &mut stages,
        )
        .await?;
    } else if scenario == "publish-retirement" {
        until(state, visit, "original primary held", |p, s| {
            !s["held"].is_null() && last(p)["finished"] == false
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
            || successor["publish"]["visible"] != false
            || last(&successor["publish"])["current"] != false
        {
            return Err("Replacement retained original Publish authority".into());
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
            return Err(format!(
                "Original Publish response receiver changed: {release}"
            ));
        }
        until(state, visit, "original Publish worker settled", |p, _| {
            last(p)["finished"] == true
        })
        .await?;
        let after = observe(state, desktop, visit).await?;
        if successor["model"] != after["model"]
            || successor["native"] != after["native"]
            || successor["server"]["requests"] != after["server"]["requests"]
            || successor["server"]["current"] != after["server"]["current"]
            || last(&after["publish"])["resolved"] != false
        {
            return Err(format!(
                "Late Publish changed successor: before={successor} after={after}"
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
    } else {
        if scenario == "publish-success" {
            until(
                state,
                visit,
                "primary ACK and held synchronization",
                |p, s| {
                    !s["held"].is_null()
                        && last(p)["slots"][0]["outcome"] == "acknowledged"
                        && last(p)["followup"]["stages"][0]["slots"][0]["outcome"] == "in-flight"
                },
            )
            .await?;
            let exercise = quiet(state, desktop, visit).await?;
            stage(
                state,
                desktop,
                name,
                "primary-ack",
                visit,
                exercise,
                &mut captures,
                &mut stages,
            )
            .await?;
        }
        let release = if scenario == "publish-success" {
            let v = original.release()?;
            if v["delivered"] != true || v["receiverRetired"] != false {
                return Err(format!("Original synchronization receiver changed: {v}"));
            }
            v
        } else {
            Value::Null
        };
        until(state, visit, "publication attempt finished", |p, _| {
            last(p)["finished"] == true && last(p)["validationOnly"] == false
        })
        .await?;
        let label = match scenario.as_str() {
            "publish-primary-rejected" => "primary-rejected",
            "publish-primary-unknown" => "primary-unconfirmed",
            "publish-sync-rejected" => "sync-rejected",
            "publish-sync-unknown" => "sync-unconfirmed",
            _ => "confirmed",
        };
        let settled = observe(state, desktop, visit).await?;
        if last(&settled["publish"])["complete"] != (label == "confirmed") {
            return Err(format!(
                "Unexpected primary/synchronization ACK boundary: {settled}"
            ));
        }
        stage(
            state,
            desktop,
            name,
            label,
            visit,
            json!({"release":release}),
            &mut captures,
            &mut stages,
        )
        .await?;
        let exercise = checked(state, desktop, visit).await?;
        let resolved = matches!(
            scenario.as_str(),
            "publish-success" | "publish-validation-recovery"
        );
        if last(&exercise["after"]["publish"])["resolved"] != resolved {
            return Err(format!("Unexpected publication resolution: {exercise}"));
        }
        if scenario == "publish-stale-readback" {
            let after = &exercise["after"];
            let readback = &last(&after["publish"])["readback"];
            let server = &after["server"]["current"]["workflow"];
            let expected_version = if case == "publish-republish-stale" {
                "V1"
            } else {
                "V2"
            };
            if readback["matches"] != false
                || server["is_published"] != true
                || server["last_published_version_id"] != expected_version
                || server["unpublished_change_count"] != 0
            {
                return Err(format!(
                    "Stale readback lost independent current server evidence: {exercise}"
                ));
            }
            if case == "publish-republish-stale" {
                let mut expected = server.clone();
                expected["is_published"] = json!(false);
                if readback["workflow"] != expected {
                    return Err(format!(
                        "Republish stale readback changed more than publication state: {exercise}"
                    ));
                }
            }
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
        if !resolved {
            let exercise = checked(state, desktop, visit).await?;
            if last(&exercise["after"]["publish"])["resolved"] != false {
                return Err(format!(
                    "Repeated observation reclassified original publication ACK: {exercise}"
                ));
            }
            stage(
                state,
                desktop,
                name,
                "checked-again",
                visit,
                exercise,
                &mut captures,
                &mut stages,
            )
            .await?;
        } else if scenario == "publish-success" {
            let before = observe(state, desktop, visit).await?;
            click(desktop, "workflow-management-back").await?;
            wait_dom(desktop,r#"({ok:!!document.getElementById('workflow-configuration')&&!document.getElementById('workflow-management')})"#).await?;
            let after = observe(state, desktop, visit).await?;
            if before["server"] != after["server"] {
                return Err("Back dispatched an unexpected request".into());
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
            let reopened = wait(state, "reopened published workflow", |v| {
                v["busy"] == false && v["route"] == "editor"
            })
            .await?;
            visit = reopened["visit"]
                .as_u64()
                .ok_or("Reopened Publish visit missing")?;
            let remount=wait_dom(desktop,r#"(()=>{const old=window.__workflowPublishOriginal,root=document.getElementById('workflow-management'),title=document.getElementById('workflow-management-title'),description=document.getElementById('workflow-management-description');return{ok:!!old&&!!root?.__workflowManagement&&!!title&&!!description&&!old.root.isConnected&&old.root!==root&&old.title!==title&&old.description!==description,originalDetached:!!old&&!old.root.isConnected,newRoot:!!old&&old.root!==root,newTitle:!!old&&old.title!==title,newDescription:!!old&&old.description!==description}})()"#).await?;
            native(desktop, true).await?;
            let after = observe(state, desktop, visit).await?;
            if !original_fields(&after)
                || after["publish"]["document"]["id"] != before["publish"]["document"]["id"]
                || !after["publish"]["document"]["pending"].is_null()
                || after["publish"]["attempts"] != before["publish"]["attempts"]
            {
                return Err(format!(
                    "Reopen changed original publication receipt: before={before} after={after}"
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
        }
    }
    let final_state = observe(state, desktop, visit).await?;
    Ok(
        json!({"ok":true,"fixture":name,"scenario":"editor","publishCase":case,"readiness":readiness,"syntheticSession":true,"noLiveNetwork":true,"aboutConfiguration":true,"viewport":stages[0]["geometry"]["viewport"],"stages":stages,"captures":captures,"final":final_state,"replacement":replacement}),
    )
}
