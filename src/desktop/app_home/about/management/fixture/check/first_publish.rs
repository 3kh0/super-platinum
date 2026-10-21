//! Finish Up through original controls; raw server receipts remain independent.
use super::*;
use crate::app_home::about::management::fixture::first_publish as endpoint;
use crate::app_home::about::management::permissions::finish_up as product;
mod actions;
mod avatar;
mod confirmation;
mod details;
mod evidence;
mod workflows;
use evidence::stage;
const LOCAL_TITLE: &str = "First publication \u{4e2d}";
const LOCAL_DESCRIPTION: &str = "Keep these first publication details.";

fn native_script(mode: &str) -> String {
    let base = include_str!("first_publish/native.js").replace("MODE", &json!(mode).to_string());
    let base = if endpoint::confirmation::requested() {
        include_str!("first_publish/confirmation/native.js").replace("BASE", &base)
    } else {
        base
    };
    let base = if crate::channel_workflows::fixture::requested() {
        include_str!("first_publish/workflows/native.js")
            .replace(
                "EXPOSURE",
                include_str!("first_publish/details/exposure.js"),
            )
            .replace("BASE", &base)
    } else {
        base
    };
    let base = if crate::channel_workflows::details::fixture::requested() {
        include_str!("first_publish/details/native.js").replace("BASE", &base)
    } else {
        base
    };
    if crate::channel_workflows::details::fixture::thread::requested() {
        include_str!("first_publish/details/thread/native.js")
            .replace("BASE", &base)
            .replace("BIND", if mode == "thread-bind" { "true" } else { "false" })
            .replace("DRAFT", &json!(crate::channel_workflows::details::fixture::thread::DRAFT).to_string())
    } else {
        base
    }
}
async fn native(desktop: &DesktopContext, mode: &str) -> Result<Value, String> {
    wait_dom(desktop, &native_script(mode)).await
}
async fn observe(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
) -> Result<Value, String> {
    let native = if endpoint::confirmation::requested() {
        evidence::observed_wait(
            state,
            desktop,
            visit,
            "original native observation",
            &native_script("observe"),
        )
        .await?
    } else {
        native(desktop, "observe").await?
    };
    let s = state.peek();
    let mut model = snapshot(&s);
    if endpoint::confirmation::requested() {
        model["confirmationDestination"] = confirmation::destination(&s);
    }
    if crate::channel_workflows::details::fixture::requested() {
        model["workflowDetails"] = crate::channel_workflows::details::snapshot(&s);
        model["detailsCustody"] = json!({"threadRoot":s.thread_root,"threadGeneration":s.thread_generation,"profileUser":s.profile_user,"groupId":s.group_panel.as_ref().map(|p|&p.id)});
    }
    if crate::channel_workflows::details::fixture::thread::requested() {
        model["detailsThread"] = crate::channel_workflows::details::fixture::thread::snapshot(&s);
    }
    Ok(
        json!({"model":model,"firstPublish":product::snapshot(&s,visit),"server":endpoint::snapshot(&s),"native":native}),
    )
}
fn last(v: &Value) -> &Value {
    v["attempts"]
        .as_array()
        .and_then(|v| v.last())
        .unwrap_or(&Value::Null)
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
    let s = state.peek();
    Err(format!(
        "{label}: product={} server={}",
        product::snapshot(&s, visit),
        endpoint::snapshot(&s)
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
            "Held first publication changed: before={before} after={after}"
        ));
    }
    Ok(json!({"before":before,"after":after,"observedMs":500}))
}
async fn checked(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
) -> Result<Value, String> {
    let before = observe(state, desktop, visit).await?;
    let reads = before["model"]["reads"]
        .as_u64()
        .ok_or("Read count absent")?;
    click(desktop, "workflow-finish-check").await?;
    wait(state, "fresh first-publication readback", |m| {
        m["busy"] == false && m["reads"].as_u64() == Some(reads + 11)
    })
    .await?;
    until(state, visit, "original first readback", |p, _| {
        !last(p)["readback"].is_null()
    })
    .await?;
    let after = evidence::settled_observe(state, desktop, visit).await?;
    if before["model"]["writes"] != after["model"]["writes"]
        || before["native"]["fields"] != after["native"]["fields"]
        || before["native"]["modal"]["fields"] != after["native"]["modal"]["fields"]
    {
        return Err("First readback changed original buffers or replayed operation".into());
    }
    Ok(json!({"before":before,"after":after}))
}

pub(super) async fn run(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    let readiness = readiness(name).await?;
    endpoint::seed(state).await?;
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
    let model = wait(state, "original first-Publish editor", |m| {
        m["route"] == "editor" && m["busy"] == false
    })
    .await?;
    let visit = model["visit"]
        .as_u64()
        .ok_or("First-Publish visit absent")?;
    native(desktop, "parent").await?;
    let original = endpoint::capture(&state.peek())?;
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
    if case != "first-default" {
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
    }
    actions::open(state, desktop, visit).await?;
    stage(
        state,
        desktop,
        name,
        "finish",
        visit,
        json!({}),
        &mut captures,
        &mut stages,
    )
    .await?;
    if case != "first-default" {
        actions::details(state, desktop, visit).await?;
        if matches!(
            case.as_str(),
            "first-settings" | "first-settings-rejected-held"
        ) {
            let exercise = actions::settings(state, desktop, visit).await?;
            stage(
                state,
                desktop,
                name,
                "selected",
                visit,
                exercise,
                &mut captures,
                &mut stages,
            )
            .await?;
        } else {
            stage(
                state,
                desktop,
                name,
                "details",
                visit,
                json!({}),
                &mut captures,
                &mut stages,
            )
            .await?;
        }
    }
    if case == "first-cancel" {
        let exercise = actions::cancel(state, desktop, visit).await?;
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
        actions::open(state, desktop, visit).await?;
        stage(
            state,
            desktop,
            name,
            "reopened-finish",
            visit,
            json!({}),
            &mut captures,
            &mut stages,
        )
        .await?;
        let exercise = actions::cancel(state, desktop, visit).await?;
        stage(
            state,
            desktop,
            name,
            "cancelled-again",
            visit,
            exercise,
            &mut captures,
            &mut stages,
        )
        .await?;
    } else if matches!(
        case.as_str(),
        "first-avatar-crop-cancel"
            | "first-avatar-crop-back"
            | "first-avatar-upload-back"
            | "first-avatar-upload-cancel-held"
            | "first-avatar-crop-back-held"
            | "first-avatar-crop-cancel-held"
    ) {
        endpoint::arm(&state.peek())?;
        avatar::run(state, desktop, name, visit, &mut captures, &mut stages).await?;
        if !matches!(
            case.as_str(),
            "first-avatar-upload-cancel-held"
                | "first-avatar-crop-back-held"
                | "first-avatar-crop-cancel-held"
        ) {
            let exercise = actions::cancel(state, desktop, visit).await?;
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
        }
    } else {
        endpoint::arm(&state.peek())?;
        if matches!(
            case.as_str(),
            "first-avatar"
                | "first-avatar-crop-retry"
                | "first-avatar-upload-retry"
                | "first-avatar-crop-drag"
                | "first-avatar-crop-drag-retry"
        ) {
            avatar::run(state, desktop, name, visit, &mut captures, &mut stages).await?;
        }
        let admission = observe(state, desktop, visit).await?;
        click(desktop, "workflow-finish-submit").await?;
        if case == "first-retirement" {
            until(state, visit, "original first publication held", |p, s| {
                s["held"].as_array().is_some_and(|v| v.len() == 1)
                    && last(p)["publication"]["outcome"] == "in-flight"
            })
            .await?;
            let exercise = quiet(state, desktop, visit).await?;
            stage(
                state,
                desktop,
                name,
                "primary-held",
                visit,
                json!({"admission":admission,"quiet":exercise}),
                &mut captures,
                &mut stages,
            )
            .await?;
            let before = observe(state, desktop, visit).await?;
            let serial = before["server"]["held"][0]
                .as_u64()
                .ok_or("Held original serial absent")? as usize;
            replacement = replace_account(state)?;
            wait_dom(desktop,r#"({ok:!!document.getElementById('workflow-configuration')&&!document.getElementById('workflow-management')&&!document.getElementById('workflow-finish-up')})"#).await?;
            let successor = observe(state, desktop, visit).await?;
            stage(
                state,
                desktop,
                name,
                "successor",
                visit,
                json!({"before":before,"after":successor,"replacement":replacement}),
                &mut captures,
                &mut stages,
            )
            .await?;
            let release = original.release(serial)?;
            if release["delivered"] != true || release["receiverRetired"] != false {
                return Err("Original held publication receiver changed".into());
            }
            until(
                state,
                visit,
                "retired first publication consumed",
                |p, _| last(p)["finished"] == true,
            )
            .await?;
            let after = observe(state, desktop, visit).await?;
            if successor["model"] != after["model"]
                || successor["native"] != after["native"]
                || successor["server"]["requests"] != after["server"]["requests"]
                || successor["server"]["current"] != after["server"]["current"]
            {
                return Err("Late first publication changed successor".into());
            }
            stage(
                state,
                desktop,
                name,
                "late-settled",
                visit,
                json!({"before":successor,"release":release,"after":after}),
                &mut captures,
                &mut stages,
            )
            .await?;
        } else {
            let mut release = Value::Null;
            if case == "first-settings-rejected-held" {
                until(
                    state,
                    visit,
                    "publication before held sibling settlement",
                    |p, s| {
                        last(p)["publication"]["outcome"] == "acknowledged"
                            && last(p)["finished"] == false
                            && s["held"].as_array().is_some_and(|v| v.len() == 1)
                            && s["settlements"]
                                .as_array()
                                .zip(s["requests"].as_array())
                                .is_some_and(|(a, b)| a.len() + 1 == b.len())
                    },
                )
                .await?;
                let exercise = quiet(state, desktop, visit).await?;
                stage(
                    state,
                    desktop,
                    name,
                    "published-while-held",
                    visit,
                    json!({"admission":admission,"quiet":exercise}),
                    &mut captures,
                    &mut stages,
                )
                .await?;
                let serial = endpoint::snapshot(&state.peek())["held"][0]
                    .as_u64()
                    .ok_or("Held run serial absent")? as usize;
                release = original.release(serial)?;
                if release["delivered"] != true || release["receiverRetired"] != false {
                    return Err("Original held settings response changed".into());
                }
            }
            until(state, visit, "first publication finished", |p, _| {
                last(p)["finished"] == true
            })
            .await?;
            if endpoint::confirmation::requested() {
                confirmation::ready(state, visit).await?;
            }
            evidence::settled_observe(state, desktop, visit).await?;
            let label = if case == "first-primary-unknown" {
                "unconfirmed"
            } else {
                "confirmed"
            };
            stage(
                state,
                desktop,
                name,
                label,
                visit,
                json!({"admission":admission,"release":release}),
                &mut captures,
                &mut stages,
            )
            .await?;
            let exercise = checked(state, desktop, visit).await?;
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
            if endpoint::confirmation::requested() {
                replacement =
                    confirmation::run(state, desktop, name, visit, &mut captures, &mut stages)
                        .await?;
            } else if matches!(
                case.as_str(),
                "first-primary-unknown" | "first-stale-readback"
            ) {
                let exercise = checked(state, desktop, visit).await?;
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
            } else {
                let before = observe(state, desktop, visit).await?;
                click(desktop, "workflow-finish-done").await?;
                wait_dom(desktop,r#"({ok:!document.getElementById('workflow-finish-up')&&!!document.getElementById('workflow-management-title')})"#).await?;
                let after = observe(state, desktop, visit).await?;
                stage(
                    state,
                    desktop,
                    name,
                    "done",
                    visit,
                    json!({"before":before,"after":after}),
                    &mut captures,
                    &mut stages,
                )
                .await?;
            }
        }
    }
    let final_state = observe(state, desktop, visit).await?;
    let setup = if crate::channel_workflows::details::fixture::requested() {
        Some(stages.drain(..8).collect::<Vec<_>>())
    } else {
        None
    };
    let mut receipt = json!({"ok":true,"fixture":name,"scenario":"editor","firstPublishCase":case,"readiness":readiness,"syntheticSession":true,"noLiveNetwork":true,"aboutConfiguration":true,"viewport":stages[0]["geometry"]["viewport"],"stages":stages,"captures":captures,"final":final_state,"replacement":replacement});
    if let Some(setup) = setup {
        receipt["setup"] = json!(setup);
    }
    if crate::channel_workflows::details::fixture::thread::requested() {
        receipt["threadCustody"] = json!(true);
    }
    Ok(receipt)
}
