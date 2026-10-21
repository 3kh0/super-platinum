//! Separate private/archived vectors; the accepted ordinary fixture stays unchanged.
use super::*;

pub(super) async fn labels(desktop: &DesktopContext, fetched: bool) -> Result<Value, String> {
    let script = include_str!("special_labels.js")
        .replace("FETCHED", &json!(fetched).to_string())
        .replace(
            "LOCK_PATH",
            &json!(crate::icons::Icon::LockFilled.path()).to_string(),
        )
        .replace(
            "ARCHIVE_PATH",
            &json!(crate::icons::Icon::Archive.path()).to_string(),
        )
        .replace(
            "FILLED_PATH",
            &json!(crate::icons::Icon::ArchiveFilled.path()).to_string(),
        )
        .replace(
            "TAG_PATH",
            &json!(crate::icons::Icon::Tag.path()).to_string(),
        );
    wait_dom(desktop, &script).await
}

pub(super) async fn target(
    desktop: &DesktopContext,
    label: &str,
    timeline: &str,
) -> Result<Value, String> {
    let name = match label {
        "archived" => "activity-archived",
        "private-archived" => "activity-private-archived",
        _ => return Err("Not an archived target".into()),
    };
    let script = include_str!("special_target.js")
        .replace("TIMELINE", &json!(timeline).to_string())
        .replace("HEADER", &json!(format!("#{name}")).to_string());
    wait_dom(desktop, &script).await
}

pub(super) async fn navigate(
    state: Signal<ShellState>,
    desktop: &DesktopContext,
    name: &str,
    stages: &mut Vec<Value>,
    captures: &mut serde_json::Map<String, Value>,
) -> Result<(), String> {
    for (id, button, opened, returned) in [
        (
            endpoint::ARCHIVED,
            "workflow-activity-channel-step-1-1",
            "archived",
            "archived-return",
        ),
        (
            endpoint::PRIVATE_ARCHIVED,
            "workflow-activity-channel-step-1-3",
            "private-archived",
            "private-archived-return",
        ),
    ] {
        click(desktop, "workflow-management-activity").await?;
        detail(state, desktop).await?;
        let original_labels = labels(desktop, false).await?;
        let before = endpoint::snapshot(&state.peek());
        click(desktop, button).await?;
        channel_wait(state, "local archived channel selected", |v| {
            v["selected"] == id
        })
        .await?;
        let after = endpoint::snapshot(&state.peek());
        if before["documents"] != after["documents"] || before["ledger"] != after["ledger"] {
            return Err("Local archived navigation changed draft or issued a read".into());
        }
        stage(
            state,
            desktop,
            name,
            opened,
            json!({"button":button,"before":before,"after":after,"originalLabels":original_labels}),
            stages,
            captures,
        )
        .await?;
        let proof = return_document(state, desktop, &before).await?;
        stage(state, desktop, name, returned, proof, stages, captures).await?;
    }
    Ok(())
}
