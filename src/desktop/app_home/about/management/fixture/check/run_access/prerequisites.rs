//! Missing identity details and the actual available-external prerequisite.
use super::*;
pub(super) async fn prepare(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    visit: u64,
    captures: &mut serde_json::Map<String, Value>,
    stages: &mut Vec<Value>,
) -> Result<(), String> {
    let cold = endpoint::case() == "run-cold-targets";
    if cold {
        until(state, visit, "initial original-target failure", |d, _| {
            let v = &d["confirmation"]["runDraft"];
            v["pending"] == false && v["notice"].is_string() && v["mode"].is_null()
        })
        .await?;
        let exercise = quiet(state, desktop, visit).await?;
        stage(
            state,
            desktop,
            name,
            "missing-offline",
            visit,
            exercise,
            captures,
            stages,
        )
        .await?;
    } else {
        stage(
            state,
            desktop,
            name,
            "permissions",
            visit,
            json!({}),
            captures,
            stages,
        )
        .await?;
    }
    let admission = observe(state, desktop, visit).await?;
    if cold {
        click(desktop, "workflow-permissions-target-retry").await?;
    } else {
        targets::select(
            state,
            desktop,
            visit,
            "workflow-permissions-run",
            "everyone",
            false,
        )
        .await?;
    }
    until(
        state,
        visit,
        "original prerequisite receiver held",
        |d, s| d["confirmation"]["runDraft"]["pending"] == true && !s["held"].is_null(),
    )
    .await?;
    let held = quiet(state, desktop, visit).await?;
    stage(
        state,
        desktop,
        name,
        if cold { "retry-held" } else { "available-held" },
        visit,
        json!({"admission":admission,"quiet":held}),
        captures,
        stages,
    )
    .await?;
    let before = observe(state, desktop, visit).await?;
    let release = endpoint::capture(&state.peek())?.release()?;
    if release["receiverRetired"] != false || release["delivered"] != true {
        return Err(format!("Original prerequisite receiver changed: {release}"));
    }
    until(state, visit, "original prerequisite adopted", |d, _| {
        let v = &d["confirmation"]["runDraft"];
        v["pending"] == false
            && v["notice"].is_null()
            && v["mode"] == if cold { "named_entities" } else { "everyone" }
    })
    .await?;
    let after = observe(state, desktop, visit).await?;
    let ids = if cold {
        json!(["UMISSING01", "CMISSING01", "TMISSING01"])
    } else {
        json!(["TRUNEXTER"])
    };
    if !draft(&after)
        || after["permissions"]["confirmation"]["runDraft"]["ids"] != ids
        || after["model"]["writes"] != 0
    {
        return Err("Original prerequisite grants or draft changed".into());
    }
    if !cold
        && after["permissions"]["confirmation"]["runDraft"]["scope"]["availableExternal"]
            != json!(["TRUNEXTER"])
    {
        return Err("Actual available external list not adopted".into());
    }
    stage(
        state,
        desktop,
        name,
        if cold { "hydrated" } else { "selected" },
        visit,
        json!({"before":before,"release":release,"after":after}),
        captures,
        stages,
    )
    .await?;
    if cold {
        let before = observe(state, desktop, visit).await?;
        targets::select(
            state,
            desktop,
            visit,
            "workflow-permissions-run",
            "everyone",
            false,
        )
        .await?;
        let after = observe(state, desktop, visit).await?;
        if before["server"] != after["server"]
            || after["permissions"]["confirmation"]["runDraft"]["ids"] != json!([])
        {
            return Err("Everyone selection dispatched or retained unexpected targets".into());
        }
        stage(
            state,
            desktop,
            name,
            "selected",
            visit,
            json!({"before":before,"after":after}),
            captures,
            stages,
        )
        .await?;
    }
    Ok(())
}
