//! Real List hover events around the original cell editor and native capture.
use super::*;

async fn action(
    desktop: &dioxus::desktop::DesktopContext,
    domain: &str,
    action: &str,
) -> Result<Value, String> {
    let script = include_str!("editor_hover.js")
        .replace("__ACTION__", &json!(action).to_string())
        .replace("__DOMAIN__", &json!(domain).to_string());
    checked(desktop, &script).await
}

pub(super) async fn hover(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    domain: &str,
) -> Result<Value, String> {
    action(desktop, domain, "hover").await?;
    wait_dom(
        desktop,
        r#"({ok:!!document.querySelector('.profile-hover')})"#,
    )
    .await?;
    let proof = action(desktop, domain, "visible").await?;
    let user = state
        .peek()
        .profile_hover
        .as_ref()
        .map(|v| v.user_id.clone());
    let expected = if domain == "user" { "U00000001" } else { "U1" };
    if user.as_deref() != Some(expected) {
        return Err(format!(
            "List hover did not retain actual {domain} source: {proof}"
        ));
    }
    Ok(json!({"user":user,"label":proof["label"],"visible":true}))
}

pub(super) async fn blocked(
    state: Signal<ShellState>,
    desktop: &dioxus::desktop::DesktopContext,
    domain: &str,
) -> Result<Value, String> {
    wait_dom(
        desktop,
        r#"({ok:!document.querySelector('.profile-hover')})"#,
    )
    .await?;
    action(desktop, domain, "remember").await?;
    let before = action(desktop, domain, "snapshot").await?;
    action(desktop, domain, "blocked").await?;
    // A bounded observation of the real event and its render, not a claim
    // about every possible scheduling delay. Original write checks follow.
    let start = std::time::Instant::now();
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let after = action(desktop, domain, "snapshot").await?;
    if before["evidence"] != after["evidence"]
        || state.peek().profile_hover.is_some()
        || !crate::slack_lists::cell_editor_blocks_profile_hover(&state.peek())
    {
        return Err(format!(
            "Hover changed original entity editor: before={before}; after={after}"
        ));
    }
    Ok(
        json!({"before":before["evidence"],"after":after["evidence"],
        "hoverAbsent":after["hoverAbsent"],"sourceConnected":after["sourceConnected"],
        "observedMs":start.elapsed().as_millis()}),
    )
}

pub(super) async fn dismiss(
    desktop: &dioxus::desktop::DesktopContext,
    domain: &str,
) -> Result<(), String> {
    action(desktop, domain, "dismiss").await?;
    wait_dom(
        desktop,
        r#"({ok:!document.querySelector('.profile-hover')})"#,
    )
    .await?;
    Ok(())
}

pub(super) async fn capture_before(
    desktop: &dioxus::desktop::DesktopContext,
    domain: &str,
) -> Result<Value, String> {
    action(desktop, domain, "remember").await?;
    Ok(action(desktop, domain, "snapshot").await?["evidence"].clone())
}

pub(super) async fn capture_after(
    desktop: &dioxus::desktop::DesktopContext,
    domain: &str,
    before: &Value,
) -> Result<Value, String> {
    let after = action(desktop, domain, "snapshot").await?["evidence"].clone();
    if before != &after {
        return Err(format!(
            "Original entity editor changed during capture: before={before}; after={after}"
        ));
    }
    Ok(json!({"before":before,"after":after}))
}
