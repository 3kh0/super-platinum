//! Actual mounted Activity clock changes. No label mutation or manual refresh.
use super::*;
use endpoint::grace_clock as source;

async fn clock_wait(
    state: Signal<ShellState>,
    predicate: impl Fn(&Value) -> bool,
) -> Result<Value, String> {
    for _ in 0..160 {
        let value = source::snapshot(&state.peek());
        if predicate(&value) {
            return Ok(value);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(format!(
        "Original Activity clock did not settle: {}",
        source::snapshot(&state.peek())
    ))
}
pub(super) async fn labels(desktop: &DesktopContext, grace: bool) -> Result<Value, String> {
    let mut vectors = shared_frozen::vectors(false);
    if !grace {
        for row in vectors.as_array_mut().unwrap() {
            match row[1].as_str().unwrap() {
                "CACTKNOWN01" => {
                    row[3] = json!("Archive");
                    row[4] = json!(crate::icons::Icon::Archive.path());
                }
                "CGRPRIV01" => {
                    row[3] = json!("ArchiveFilled");
                    row[4] = json!(crate::icons::Icon::ArchiveFilled.path());
                }
                "CGRARCH01" => {
                    row[2] = json!("grace-archived (archived)");
                    row[3] = json!("Archive");
                    row[4] = json!(crate::icons::Icon::Archive.path());
                }
                "CGRPRAR01" => {
                    row[2] = json!("grace-private-archived (archived)");
                    row[3] = json!("ArchiveFilled");
                    row[4] = json!(crate::icons::Icon::ArchiveFilled.path());
                }
                _ => {}
            }
        }
    }
    let original = include_str!("special_labels.js");
    let start = original.find(" const vectors=").unwrap();
    let end = original.find(" const rect=").unwrap();
    let script = format!(
        "{} const vectors={};\n{}",
        original[..start].replace("FETCHED", "false"),
        vectors,
        &original[end..]
    );
    let mut proof = wait_dom(desktop, &script).await?;
    proof["identity"] = wait_dom(desktop, include_str!("grace_identity.js")).await?;
    Ok(proof)
}
async fn checkpoint(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    label: &str,
    grace: Option<bool>,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    let native = if let Some(grace) = grace {
        labels(desktop, grace).await?
    } else {
        wait_dom(desktop,r#"(()=>{
const saved=window.__activityGraceIdentity,activity=document.getElementById('workflow-activity'),detail=document.getElementById('workflow-activity-detail');
if(!saved)return{ok:false};const rows=saved.buttons.map((e,i)=>({id:e.id,connected:e.isConnected,nameConnected:saved.names[i].isConnected}));
return{ok:!activity&&!detail&&!saved.detail.isConnected&&rows.every(r=>!r.connected&&!r.nameConnected),activityPresent:!!activity,detailPresent:!!detail,originalDetailConnected:saved.detail.isConnected,originalRootConnected:saved.root.isConnected,rows};
})()"#).await?
    };
    stage(
        state,
        desktop,
        name,
        label,
        json!({"clock":source::snapshot(&state.peek()),"native":native}),
        stages,
        captures,
    )
    .await
}
fn held_id(value: &Value) -> Option<usize> {
    value["projections"]
        .as_array()?
        .iter()
        .position(|p| p["held"] == true && p["finished"] == false)
}
pub(super) async fn finish(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    readiness: Value,
    mut stages: Vec<Value>,
    mut captures: serde_json::Map<String, Value>,
) -> Result<Value, String> {
    // Establish original references before any clock update. SVG paths may change.
    let original = labels(desktop, true).await?;
    stages.last_mut().unwrap()["proof"]["graceClock"] =
        json!({"clock":source::snapshot(&state.peek()),"native":original});
    for (label, now) in [
        ("grace-equal", source::UTC_END),
        ("grace-after", source::UTC_END + 1),
    ] {
        source::set(&state.peek(), now, false, false)?;
        labels(desktop, false).await?;
        checkpoint(
            state,
            desktop,
            name,
            label,
            Some(false),
            &mut stages,
            &mut captures,
        )
        .await?;
    }
    source::set(&state.peek(), source::BETWEEN, false, true)?;
    let held = clock_wait(state, |v| v["held"] == true).await?;
    let id = held_id(&held).ok_or("Missing original held projection")?;
    let generation = held["projections"][id]["generation"].as_u64().unwrap();
    checkpoint(
        state,
        desktop,
        name,
        "zone-held",
        Some(true),
        &mut stages,
        &mut captures,
    )
    .await?;
    source::set(&state.peek(), source::BETWEEN, true, false)?;
    clock_wait(state, |v| {
        v["labels"]["clockGeneration"]
            .as_u64()
            .is_some_and(|g| g > generation)
    })
    .await?;
    checkpoint(
        state,
        desktop,
        name,
        "zone-invalidated",
        Some(true),
        &mut stages,
        &mut captures,
    )
    .await?;
    source::release(&state.peek())?;
    clock_wait(state, |v| {
        v["projections"][id]["finished"] == true
            && v["projections"][id]["applied"] == false
            && v["projections"].as_array().is_some_and(|rows| {
                rows.iter()
                    .skip(id + 1)
                    .any(|p| p["applied"] == true && p["sample"]["eastern"] == true)
            })
    })
    .await?;
    checkpoint(
        state,
        desktop,
        name,
        "zone-refreshed",
        Some(false),
        &mut stages,
        &mut captures,
    )
    .await?;
    source::set(&state.peek(), source::BETWEEN, true, true)?;
    let held = clock_wait(state, |v| v["held"] == true).await?;
    let left_id = held_id(&held).ok_or("Missing leave projection")?;
    checkpoint(
        state,
        desktop,
        name,
        "leave-held",
        Some(false),
        &mut stages,
        &mut captures,
    )
    .await?;
    click(desktop, "workflow-activity-back").await?;
    wait(state, "original dirty editor returned", |v| {
        v["activity"]["active"] == false && exact(v)
    })
    .await?;
    checkpoint(
        state,
        desktop,
        name,
        "activity-left",
        None,
        &mut stages,
        &mut captures,
    )
    .await?;
    source::release(&state.peek())?;
    let settled = clock_wait(state, |v| {
        v["projections"][left_id]["finished"] == true
            && v["projections"][left_id]["applied"] == false
    })
    .await?;
    source::set(&state.peek(), source::UTC_END - 1, false, false)?;
    let quiet_started = std::time::Instant::now();
    tokio::time::sleep(Duration::from_millis(1250)).await;
    let quiet_elapsed = quiet_started.elapsed().as_millis();
    if source::snapshot(&state.peek())["projections"] != settled["projections"] {
        return Err("Unmounted Activity initiated another projection".into());
    }
    checkpoint(
        state,
        desktop,
        name,
        "clock-left",
        None,
        &mut stages,
        &mut captures,
    )
    .await?;
    stages.last_mut().unwrap()["proof"]["exercise"]["quiet"] = json!({"elapsedMillis":quiet_elapsed,
        "before":settled["projections"],"after":source::snapshot(&state.peek())["projections"]});
    Ok(
        json!({"ok":true,"fixture":name,"case":case(),"graceClock":true,"readiness":readiness,
        "viewport":stages.last().unwrap()["geometry"]["viewport"],"aboutConfiguration":true,"actualActivityEntry":true,
        "noLiveNetwork":true,"stages":stages,"captures":captures,"final":snapshot(&state.peek()),"clock":source::snapshot(&state.peek())}),
    )
}
