use super::*;
pub(super) async fn shot(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    stage: &str,
    exercise: Value,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    let proof = json!({"source":source::snapshot(&state.peek()),"cache":rich::files::fixture_status(&state.peek(),file::ID),"exercise":exercise});
    capture(state, desktop, name, stage, proof, stages, captures).await
}
pub(super) async fn held_file() -> Result<(), String> {
    for _ in 0..100 {
        if file::status()["held"] == true {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err("Expected held group title file read".into())
}
pub(super) async fn no_editor(desktop: &DesktopContext) -> Result<(), String> {
    wait_dom(desktop,"({ok:!document.getElementById('profile-section-rich-pane')&&!!document.getElementById('profile-section-field-long')})").await.map(|_|())
}
pub(super) async fn request(state: Signal<ShellState>, kind: &str) -> Result<u64, String> {
    for _ in 0..120 {
        let pending = transport::pending();
        if pending.len() > 1 {
            return Err("Duplicate nested group read".into());
        }
        if let Some(row) = pending.first() {
            let serial = row["serial"].as_u64().ok_or("Missing request serial")?;
            if row["kind"] == "discovery" {
                transport::release(
                    serial,
                    Ok(
                        json!({"ok":true,"url":format!("https://edgeapi.slack.com/cache/{}",row["team"].as_str().unwrap()),"ttl_seconds":300}),
                    ),
                )?;
            } else {
                if row["kind"] != kind {
                    return Err(format!("Wrong group route: {row}"));
                }
                if kind == "directory" && row["body"] != json!({"ids":[source::GROUP]}) {
                    return Err("Group read escaped nested ID".into());
                }
                if kind == "direct"
                    && (row["body"]["usergroups"] != source::GROUP
                        || row["body"]["include_users"] != "false"
                        || row["body"]["include_count"] != "true"
                        || row["body"]["include_disabled"] != "true"
                        || row["body"]["_x_reason"] != "fallback:fetchRawUserGroupsById")
                {
                    return Err("Wrong nested group fallback".into());
                }
                return Ok(serial);
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(format!(
        "Expected group {kind}: {}",
        source::snapshot(&state.peek())
    ))
}
pub(super) async fn settled(state: Signal<ShellState>, finished: u64) -> Result<(), String> {
    for _ in 0..100 {
        let s = source::snapshot(&state.peek());
        let managers = s["unknown"]["managers"]
            .as_array()
            .ok_or("Missing manager evidence")?;
        if s["unknown"]["quiescent"] == true
            && managers.iter().any(|m| {
                m["current"] == true && m["finished"] == finished && m["pendingIds"] == json!([])
            })
            && transport::pending().is_empty()
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(format!(
        "Missing-group acquisition did not settle: {}",
        source::snapshot(&state.peek())
    ))
}
pub(super) async fn quiet(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    text: &str,
    label: &str,
) -> Result<(), String> {
    let before = transport::requests();
    for _ in 0..5 {
        editor_label(desktop, text, true, label).await?;
        wait_dom(desktop,"({ok:!document.getElementById('profile-section-rich-status')?.textContent&&!document.getElementById('status-toast')})").await?;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    if before != transport::requests() {
        return Err(format!(
            "Unexpected group refetch: {}",
            source::snapshot(&state.peek())
        ));
    }
    Ok(())
}
