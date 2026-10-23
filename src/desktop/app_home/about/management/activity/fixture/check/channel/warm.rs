//! Original rendered labels and dirty draft after a real private cache reload.
use super::*;
use crate::app_home::channel_fetches::plan_info;
use endpoint::warm as source;

fn installed(s: &ShellState, live: &plan_info::Snapshot) -> Option<Value> {
    if !live.current(s) {
        return None;
    }
    // snapshot() calls production observed()/Input.current(). In particular,
    // an old equal-value projection cannot pass a new positive source revision.
    let projection = endpoint::snapshot(s)["labels"].clone();
    (projection["current"] == true
        && projection["planProvenance"] == "live"
        && projection.get("plan") == Some(live.raw()))
    .then_some(projection)
}
fn same_dom_rows(native: &Value) -> bool {
    let (Some(rows), Some(identity)) = (
        native["rows"].as_array(),
        native["identity"]["rows"].as_array(),
    ) else {
        return false;
    };
    rows.len() == 7
        && identity.len() == rows.len()
        && rows.iter().zip(identity).all(|(a, b)| {
            ["id", "channel", "text"]
                .iter()
                .all(|key| a.get(*key).is_some() && a.get(*key) == b.get(*key))
        })
}
async fn settled(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    grace: bool,
) -> Result<(Value, Value), String> {
    let live = plan_info::capture(&state.peek(), "TWORKFLOW")
        .ok_or("Original positive PlanInfo absent")?;
    for _ in 0..100 {
        if !live.current(&state.peek()) {
            return Err("Original positive PlanInfo replaced during capture".into());
        }
        let ready = installed(&state.peek(), &live).is_some();
        if ready {
            let native = grace_clock::labels(desktop, grace).await?;
            if let Some(projection) = installed(&state.peek(), &live)
                && same_dom_rows(&native)
            {
                return Ok((native, projection));
            }
        }
        // Predicate-driven wait only; the original action is never replayed.
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!(
        "Original live PlanInfo label projection did not settle: {}",
        endpoint::snapshot(&state.peek())["labels"]
    ))
}
pub(super) async fn finish(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    readiness: Value,
    mut stages: Vec<Value>,
    mut captures: serde_json::Map<String, Value>,
) -> Result<Value, String> {
    let native = grace_clock::labels(desktop, true).await?;
    stages.last_mut().unwrap()["proof"]["warmNative"] = native;
    for label in [
        "empty-positive",
        "null-positive",
        "foreign-refused",
        "late-refused",
    ] {
        source::action(state, label)?;
        let expected_grace = label == "empty-positive";
        let (native, projection) = settled(state, desktop, expected_grace).await?;
        let proof = source::snapshot(&state.peek());
        if proof["display"]["provenance"] != "live" || proof["oldDisplayCurrent"] != false {
            return Err(format!(
                "Positive PlanInfo did not replace retained witness: {proof}"
            ));
        }
        stage(
            state,
            desktop,
            name,
            label,
            json!({"native":native,"projection":projection}),
            &mut stages,
            &mut captures,
        )
        .await?;
    }
    click(desktop, "workflow-activity-back").await?;
    wait(state, "original dirty editor returned", |v| {
        v["activity"]["active"] == false && exact(v)
    })
    .await?;
    let detached=wait_dom(desktop,r#"(()=>{const p=window.__activityGraceIdentity;
const title=document.getElementById('workflow-management-title'),description=document.getElementById('workflow-management-description');
return{ok:!!p&&!p.detail.isConnected&&p.buttons.every(e=>!e.isConnected)&&!!title&&!!description,
originalRootConnected:!!p?.root.isConnected,originalDetailConnected:!!p?.detail.isConnected,
title:title?.value,description:description?.value};})()"#).await?;
    stage(
        state,
        desktop,
        name,
        "returned",
        json!({"native":detached}),
        &mut stages,
        &mut captures,
    )
    .await?;
    Ok(
        json!({"ok":true,"fixture":name,"case":case(),"planInfoWarm":source::snapshot(&state.peek()),
        "readiness":readiness,"viewport":stages.last().unwrap()["geometry"]["viewport"],
        "aboutConfiguration":true,"actualActivityEntry":true,"noLiveNetwork":true,
        "stages":stages,"captures":captures,"final":snapshot(&state.peek())}),
    )
}
