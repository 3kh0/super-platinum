//! Agent eligibility uses original controls, producer replies and real held receivers.
use super::*;
mod retirement;

pub(super) async fn run(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
) -> Result<Value, String> {
    let readiness = readiness(name).await?;
    endpoint::seed(state).await?;
    let case = endpoint::case();
    let mut captures = serde_json::Map::new();
    let mut stages = vec![];
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
    let (model, entry) = actions::enter(state, desktop).await?;
    if !entry.is_null() {
        return Err("Agent cases must use original Configuration entry".into());
    }
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
    actions::open(state, desktop, visit).await?;
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
    let admission = query(state, desktop, visit, "manager").await?;
    match case.as_str() {
        "managers-agents" => {
            ready(state, visit).await?;
            let eligible = observe(state, desktop, visit).await?;
            matrix(&eligible)?;
            stage(
                state,
                desktop,
                name,
                "eligible",
                visit,
                json!({"admission":admission,"after":eligible}),
                &mut captures,
                &mut stages,
            )
            .await?;
            let mut choices = vec![];
            for id in ["UMANNEW01", "UMANAFYES", "UMANAFDEF"] {
                choices.push(actions::target(state, desktop, visit, id, true).await?);
            }
            choices.push(actions::target(state, desktop, visit, "UMANOLD01", false).await?);
            let selected = observe(state, desktop, visit).await?;
            if selected["permissions"]["confirmation"]["managerDraft"]["ids"]
                != json!(["UWORKFLOW", "UMANNEW01", "UMANAFYES", "UMANAFDEF"])
                || selected["permissions"]["confirmation"]["runDraft"]["ids"]
                    != json!([
                        "UWORKFLOW",
                        "UMANNEW01",
                        "UMANAFYES",
                        "UMANAFDEF",
                        "UMANOLD01"
                    ])
            {
                return Err(format!(
                    "Actual agent manager/run selection differs: {selected}"
                ));
            }
            stage(
                state,
                desktop,
                name,
                "selected",
                visit,
                json!({"choices":choices}),
                &mut captures,
                &mut stages,
            )
            .await?;
            let admission = observe(state, desktop, visit).await?;
            endpoint::arm(&state.peek())?;
            click(desktop, "workflow-permissions-save").await?;
            until(state, visit, "agent manager/run ACKs", |d, _| {
                d["attempts"][0]["finished"] == true
            })
            .await?;
            if product::snapshot(&state.peek(), visit)["attempts"][0]["complete"] != true {
                return Err(format!(
                    "Agent manager/run ACKs incomplete: product={} server={}",
                    product::snapshot(&state.peek(), visit),
                    endpoint::snapshot(&state.peek())
                ));
            }
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
            if exercise["after"]["permissions"]["attempts"][0]["resolved"] != true {
                return Err("Agent readback did not resolve".into());
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
            actions::back_reopen(state, desktop, name, &mut visit, &mut captures, &mut stages)
                .await?;
        }
        "managers-agents-recovery" => {
            until(state, visit, "actual malformed app fact refused", |d, _| {
                let m = &d["confirmation"]["managerDraft"];
                m["pending"] == false && m["agentFacts"]["status"] == "failed"
            })
            .await?;
            let quiet_observation = quiet(state, desktop, visit).await?;
            wait_dom(desktop,r#"({ok:!!document.getElementById('workflow-permissions-manager-agent-retry')&&!document.getElementById('workflow-permissions-manager-agent-retry').disabled})"#).await?;
            stage(
                state,
                desktop,
                name,
                "facts-failed",
                visit,
                json!({"admission":admission,"quiet":quiet_observation}),
                &mut captures,
                &mut stages,
            )
            .await?;
            let before = observe(state, desktop, visit).await?;
            click(desktop, "workflow-permissions-manager-agent-retry").await?;
            held(state, visit).await?;
            let quiet_observation = quiet(state, desktop, visit).await?;
            stage(
                state,
                desktop,
                name,
                "retry-held",
                visit,
                json!({"admission":before,"quiet":quiet_observation}),
                &mut captures,
                &mut stages,
            )
            .await?;
            let before = observe(state, desktop, visit).await?;
            let release = endpoint::capture(&state.peek())?.release()?;
            if release["delivered"] != true || release["receiverRetired"] != false {
                return Err(format!("Actual retry receiver changed: {release}"));
            }
            ready(state, visit).await?;
            let after = observe(state, desktop, visit).await?;
            matrix(&after)?;
            if !draft(&after) || after["model"]["writes"] != 0 {
                return Err("Fact recovery changed original draft or wrote".into());
            }
            stage(
                state,
                desktop,
                name,
                "recovered",
                visit,
                json!({"before":before,"release":release,"after":after}),
                &mut captures,
                &mut stages,
            )
            .await?;
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
        }
        _ => {
            held(state, visit).await?;
            let quiet_observation = quiet(state, desktop, visit).await?;
            stage(
                state,
                desktop,
                name,
                "facts-held",
                visit,
                json!({"admission":admission,"quiet":quiet_observation}),
                &mut captures,
                &mut stages,
            )
            .await?;
            replacement =
                retirement::run(state, desktop, name, visit, &mut captures, &mut stages).await?;
        }
    }
    let final_state = observe(state, desktop, visit).await?;
    Ok(
        json!({"ok":true,"fixture":name,"scenario":"editor","managersCase":case,"readiness":readiness,"syntheticSession":true,"noLiveNetwork":true,"aboutConfiguration":true,"viewport":stages[0]["geometry"]["viewport"],"stages":stages,"captures":captures,"final":final_state,"replacement":replacement}),
    )
}

async fn query(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    visit: u64,
    text: &str,
) -> Result<Value, String> {
    let before = observe(state, desktop, visit).await?;
    let confirmation = before["permissions"]["confirmation"]["id"].clone();
    let generation = before["permissions"]["confirmation"]["managerDraft"]["generation"]
        .as_u64()
        .ok_or("Original query generation missing")?;
    input(desktop, "workflow-permissions-manager-query", text).await?;
    until(state, visit, "actual manager query event", |d, _| {
        let c = &d["confirmation"];
        let m = &c["managerDraft"];
        c["id"] == confirmation
            && m["generation"].as_u64().is_some_and(|v| v > generation)
            && m["query"] == text
    })
    .await?;
    Ok(before)
}
async fn held(state: Signal<ShellState>, visit: u64) -> Result<(), String> {
    until(state, visit, "actual original app fact held", |d, s| {
        d["confirmation"]["managerDraft"]["pending"] == true
            && d["confirmation"]["managerDraft"]["agentFacts"]["status"] == "loading"
            && !s["held"].is_null()
    })
    .await
}
async fn ready(state: Signal<ShellState>, visit: u64) -> Result<(), String> {
    until(
        state,
        visit,
        "actual complete acquired eligibility",
        |d, _| {
            d["confirmation"]["managerDraft"]["pending"] == false
                && d["confirmation"]["managerDraft"]["agentFacts"]["status"] == "ready"
        },
    )
    .await
}
fn matrix(observation: &Value) -> Result<(), String> {
    let m = &observation["permissions"]["confirmation"]["managerDraft"];
    let rows = m["rows"].as_array().ok_or("Actual manager rows missing")?;
    for (id, eligible) in [
        ("UMANNEW01", true),
        ("UMANBOT01", false),
        ("UMANAFYES", true),
        ("UMANAFNO1", false),
        ("UMANAFDEF", true),
        ("UMANAFDIS", false),
        ("UMANAFBAD", false),
    ] {
        let row = rows
            .iter()
            .find(|r| r["id"] == id)
            .ok_or_else(|| format!("Original acquired row {id} absent"))?;
        let shown = m["results"]
            .as_array()
            .is_some_and(|r| r.iter().any(|v| v == id));
        if row["eligible"] != eligible
            || shown != eligible
            || row["eligibility"] != if eligible { "allowed" } else { "excluded" }
        {
            return Err(format!("Actual eligibility mismatch {id}: {m}"));
        }
    }
    Ok(())
}
