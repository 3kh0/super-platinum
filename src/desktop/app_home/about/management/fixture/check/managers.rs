//! Real manager controls with original raw response and native buffer custody.
use super::*;
use crate::app_home::about::management::fixture::managers as endpoint;
use crate::app_home::about::management::permissions as product;
mod actions;
mod agents;
mod evidence;
mod lifecycle;
use evidence::{checked, draft, native, observe, quiet, stage, until};
pub(super) async fn run(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    if endpoint::agents::active() {
        return agents::run(state, desktop, name).await;
    }
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
    let (model, management_entry) = actions::enter(state, desktop).await?;
    let mut visit = model["visit"]
        .as_u64()
        .ok_or("Original manager visit absent")?;
    native(desktop, true).await?;
    stage(
        state,
        desktop,
        name,
        "editor",
        visit,
        if management_entry.is_null() {
            json!({})
        } else {
            json!({"managementEntry":management_entry})
        },
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
    actions::open(state, desktop, visit).await?;
    if case == "managers-cold" {
        lifecycle::cold(state, desktop, name, visit, &mut captures, &mut stages).await?;
        actions::back_reopen(state, desktop, name, &mut visit, &mut captures, &mut stages).await?;
    } else {
        if matches!(case.as_str(), "managers-success" | "managers-no-trigger") {
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
        }
        if case == "managers-success" {
            let exercise =
                actions::close_local(state, desktop, visit, "workflow-permissions-save").await?;
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
            actions::open(state, desktop, visit).await?;
            let first = actions::target(state, desktop, visit, "UWORKFLOW", false).await?;
            let second = actions::target(state, desktop, visit, "UMANOLD01", false).await?;
            wait_dom(desktop,r#"({ok:document.getElementById('workflow-permissions-save')?.disabled===true&&[...document.querySelectorAll('#workflow-permissions-manager-settings p')].some(e=>e.textContent==='You must provide at least 1 workflow manager')})"#).await?;
            stage(
                state,
                desktop,
                name,
                "empty",
                visit,
                json!({"removals":[first,second]}),
                &mut captures,
                &mut stages,
            )
            .await?;
            actions::query(state, desktop, visit, "", "UWORKFLOW").await?;
            actions::target(state, desktop, visit, "UWORKFLOW", true).await?;
            actions::target(state, desktop, visit, "UMANOLD01", true).await?;
        }
        if case == "managers-combined" {
            stage(
                state,
                desktop,
                name,
                "named",
                visit,
                json!({}),
                &mut captures,
                &mut stages,
            )
            .await?;
        }
        let choice = actions::chosen(state, desktop, visit).await?;
        stage(
            state,
            desktop,
            name,
            "selected",
            visit,
            choice,
            &mut captures,
            &mut stages,
        )
        .await?;
        if case == "managers-success" {
            let exercise =
                actions::close_local(state, desktop, visit, "workflow-permissions-cancel").await?;
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
            actions::chosen(state, desktop, visit).await?;
        }
        if endpoint::self_remove() {
            lifecycle::self_removal(state, desktop, name, visit, &mut captures, &mut stages)
                .await?;
        } else {
            let admission = observe(state, desktop, visit).await?;
            endpoint::arm(&state.peek())?;
            let original = endpoint::capture(&state.peek())?;
            click(desktop, "workflow-permissions-save").await?;
            if case == "managers-retirement" {
                replacement = lifecycle::retirement(
                    state,
                    desktop,
                    name,
                    visit,
                    admission,
                    original,
                    &mut captures,
                    &mut stages,
                )
                .await?;
            } else {
                until(state, visit, "original manager attempt finished", |d, _| {
                    d["attempts"][0]["finished"] == true
                })
                .await?;
                let success = matches!(
                    case.as_str(),
                    "managers-success" | "managers-combined" | "managers-no-trigger"
                );
                if success
                    && product::snapshot(&state.peek(), visit)["attempts"][0]["complete"] != true
                {
                    let s = state.peek();
                    let product = product::snapshot(&s, visit);
                    let mut server = endpoint::snapshot(&s);
                    // Initial producer seed is already preserved in the fixture packet;
                    // retain actual plans, requests and outcomes for this failed attempt.
                    if let Some(fields) = server.as_object_mut() {
                        fields.remove("seed");
                    }
                    return Err(format!(
                        "Manager mutation did not receive all required ACKs: product={product} server={server}"
                    ));
                }
                let label = if success {
                    "acknowledged"
                } else if case == "managers-add-rejected" {
                    "rejected"
                } else {
                    "unconfirmed"
                };
                stage(
                    state,
                    desktop,
                    name,
                    label,
                    visit,
                    json!({"admission":admission}),
                    &mut captures,
                    &mut stages,
                )
                .await?;
                let exercise = checked(state, desktop, visit).await?;
                if success && exercise["after"]["permissions"]["attempts"][0]["resolved"] != true {
                    return Err("Exact manager readback did not resolve complete attempt".into());
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
                if success {
                    actions::back_reopen(
                        state,
                        desktop,
                        name,
                        &mut visit,
                        &mut captures,
                        &mut stages,
                    )
                    .await?;
                } else {
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
                }
            }
        }
    }
    let final_state = observe(state, desktop, visit).await?;
    Ok(
        json!({"ok":true,"fixture":name,"scenario":"editor","managersCase":case,"readiness":readiness,"syntheticSession":true,"noLiveNetwork":true,"aboutConfiguration":true,"viewport":stages[0]["geometry"]["viewport"],"stages":stages,"captures":captures,"final":final_state,"replacement":replacement}),
    )
}
