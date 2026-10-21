use super::*;
pub(super) async fn cold(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    visit: u64,
    captures: &mut serde_json::Map<String, Value>,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    until(state, visit, "original manager hydration failed", |d, _| {
        let m = &d["confirmation"]["managerDraft"];
        m["pending"] == false && m["notice"].is_string()
    })
    .await?;
    let offline_observation = quiet(state, desktop, visit).await?;
    wait_dom(desktop,r#"({ok:document.getElementById('workflow-permissions-save')?.disabled===false&&!!document.getElementById('workflow-permissions-manager-retry')})"#).await?;
    stage(
        state,
        desktop,
        name,
        "missing-offline",
        visit,
        offline_observation,
        captures,
        stages,
    )
    .await?;
    let admission = observe(state, desktop, visit).await?;
    click(desktop, "workflow-permissions-manager-retry").await?;
    until(state, visit, "original manager response held", |d, s| {
        d["confirmation"]["managerDraft"]["pending"] == true && !s["held"].is_null()
    })
    .await?;
    let quiet = quiet(state, desktop, visit).await?;
    stage(
        state,
        desktop,
        name,
        "retry-held",
        visit,
        json!({"admission":admission,"quiet":quiet}),
        captures,
        stages,
    )
    .await?;
    let before = observe(state, desktop, visit).await?;
    let release = endpoint::capture(&state.peek())?.release()?;
    if release["delivered"] != true || release["receiverRetired"] != false {
        return Err(format!("Original hydration receiver changed: {release}"));
    }
    until(state, visit, "actual manager raw adopted", |d, _| {
        let m = &d["confirmation"]["managerDraft"];
        m["pending"] == false
            && m["notice"].is_null()
            && m["rows"]
                .as_array()
                .is_some_and(|v| v.iter().any(|r| r["id"] == "UMANCOLD1"))
    })
    .await?;
    let after = observe(state, desktop, visit).await?;
    if !draft(&after) || after["model"]["writes"] != 0 {
        return Err("Manager hydration wrote or lost native draft".into());
    }
    stage(
        state,
        desktop,
        name,
        "hydrated",
        visit,
        json!({"before":before,"release":release,"after":after}),
        captures,
        stages,
    )
    .await?;
    let cancelled =
        actions::close_local(state, desktop, visit, "workflow-permissions-cancel").await?;
    stage(
        state,
        desktop,
        name,
        "cancelled",
        visit,
        cancelled,
        captures,
        stages,
    )
    .await
}
pub(super) async fn retirement(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    visit: u64,
    admission: Value,
    original: endpoint::Model,
    captures: &mut serde_json::Map<String, Value>,
    stages: &mut Vec<Value>,
) -> Result<Value, String> {
    until(state, visit, "original add held", |d, s| {
        !s["held"].is_null() && d["attempts"][0]["finished"] == false
    })
    .await?;
    let quiet = quiet(state, desktop, visit).await?;
    stage(
        state,
        desktop,
        name,
        "held",
        visit,
        json!({"admission":admission,"quiet":quiet}),
        captures,
        stages,
    )
    .await?;
    let before = observe(state, desktop, visit).await?;
    let replacement = replace_account(state)?;
    wait_dom(desktop,r#"({ok:!!document.getElementById('workflow-configuration')&&!document.getElementById('workflow-management')})"#).await?;
    let successor = observe(state, desktop, visit).await?;
    if successor["server"]["ownerCurrent"] != false
        || successor["permissions"]["visible"] != false
        || successor["permissions"]["attempts"][0]["current"] != false
    {
        return Err("Replacement retained original manager authority".into());
    }
    stage(
        state,
        desktop,
        name,
        "successor",
        visit,
        json!({"replacement":replacement,"before":before,"after":successor}),
        captures,
        stages,
    )
    .await?;
    let release = original.release()?;
    if release["delivered"] != true || release["receiverRetired"] != false {
        return Err(format!("Original add receiver changed: {release}"));
    }
    until(state, visit, "original manager worker finished", |d, _| {
        d["attempts"][0]["finished"] == true
    })
    .await?;
    let after = observe(state, desktop, visit).await?;
    if successor["model"] != after["model"]
        || successor["native"] != after["native"]
        || !retained_finish(&successor["retainedDocument"], &after["retainedDocument"])
        || successor["server"]["requests"] != after["server"]["requests"]
        || successor["server"]["current"] != after["server"]["current"]
        || after["permissions"]["attempts"][0]["resolved"] != false
    {
        return Err(format!(
            "Late manager add changed successor or dispatched remove: before={successor} after={after}"
        ));
    }
    stage(
        state,
        desktop,
        name,
        "late-settled",
        visit,
        json!({"before":successor,"release":release,"after":after,"successorUnchanged":true}),
        captures,
        stages,
    )
    .await?;
    Ok(replacement)
}
pub(super) async fn self_removal(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    visit: u64,
    captures: &mut serde_json::Map<String, Value>,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    let before = observe(state, desktop, visit).await?;
    click(desktop, "workflow-permissions-save").await?;
    until(state, visit, "self removal speed bump", |d, _| {
        d["confirmation"]["managerDraft"]["speedBump"] == true
    })
    .await?;
    let after = observe(state, desktop, visit).await?;
    if before["server"] != after["server"] || !draft(&after) {
        return Err("Self-removal confirmation dispatched or changed draft".into());
    }
    stage(
        state,
        desktop,
        name,
        "confirmation",
        visit,
        json!({"before":before,"after":after}),
        captures,
        stages,
    )
    .await?;
    if endpoint::case() == "managers-self-success" {
        let before = observe(state, desktop, visit).await?;
        click(desktop, "workflow-permissions-self-cancel").await?;
        until(state, visit, "self removal cancelled", |d, _| {
            d["confirmation"]["managerDraft"]["speedBump"] == false
        })
        .await?;
        let after = observe(state, desktop, visit).await?;
        if before["server"] != after["server"] || !draft(&after) {
            return Err("Speed bump Cancel dispatched or changed draft".into());
        }
        stage(
            state,
            desktop,
            name,
            "cancelled-confirmation",
            visit,
            json!({"before":before,"after":after}),
            captures,
            stages,
        )
        .await?;
        click(desktop, "workflow-permissions-save").await?;
        until(state, visit, "original speed bump reopened", |d, _| {
            d["confirmation"]["managerDraft"]["speedBump"] == true
        })
        .await?;
    }
    let admission = observe(state, desktop, visit).await?;
    endpoint::arm(&state.peek())?;
    let original = endpoint::capture(&state.peek())?;
    click(desktop, "workflow-permissions-self-save").await?;
    until(
        state,
        visit,
        "intentional original manager departure",
        |d, s| {
            d["attempts"][0]["departure"]["departed"] == true
                && (endpoint::case() == "managers-self-success" || !s["held"].is_null())
        },
    )
    .await?;
    let departed = observe(state, desktop, visit).await?;
    if departed["model"]["route"] != "managed"
        || !departed["permissions"]["document"].is_null()
        || departed["retainedDocument"]["title"] != TITLE
        || departed["retainedDocument"]["description"] != DESCRIPTION
    {
        return Err(format!(
            "Self-removal did not preserve original retained draft: {departed}"
        ));
    }
    stage(
        state,
        desktop,
        name,
        if endpoint::case() == "managers-self-success" {
            "departed"
        } else {
            "departed-held"
        },
        visit,
        json!({"admission":admission,"departure":departed}),
        captures,
        stages,
    )
    .await?;
    let exercise = if endpoint::case() == "managers-self-rejected-held" {
        let before = observe(state, desktop, visit).await?;
        let release = original.release()?;
        if release["delivered"] != true || release["receiverRetired"] != false {
            return Err(format!("Original run receiver changed: {release}"));
        }
        until(state, visit, "original run follow-up finished", |d, s| {
            d["attempts"][0]["finished"] == true
                && s["requests"]
                    .as_array()
                    .is_some_and(|r| r.iter().any(|v| v["phase"] == "followup"))
        })
        .await?;
        let after = observe(state, desktop, visit).await?;
        let mut expected_model = before["model"].clone();
        expected_model["reads"] = json!(
            before["model"]["reads"]
                .as_u64()
                .ok_or("Original read count absent")?
                + 1
        );
        if expected_model != after["model"]
            || before["native"] != after["native"]
            || !retained_finish(&before["retainedDocument"], &after["retainedDocument"])
            || after["permissions"]["attempts"][0]["resolved"] != false
        {
            return Err(format!(
                "Original follow-up altered departed route/draft: before={before} after={after}"
            ));
        }
        json!({"before":before,"release":release,"after":after})
    } else {
        until(state, visit, "self-removal attempt settled", |d, _| {
            d["attempts"][0]["finished"] == true
        })
        .await?;
        quiet(state, desktop, visit).await?
    };
    stage(
        state,
        desktop,
        name,
        if endpoint::case() == "managers-self-success" {
            "settled"
        } else {
            "late-settled"
        },
        visit,
        exercise,
        captures,
        stages,
    )
    .await
}

fn retained_finish(before: &Value, after: &Value) -> bool {
    let mut expected = before.clone();
    let old = before["revision"].as_u64();
    let new = after["revision"].as_u64();
    if old.zip(new).is_none_or(|(a, b)| b <= a) {
        return false;
    }
    expected["revision"] = after["revision"].clone();
    expected == *after
}
