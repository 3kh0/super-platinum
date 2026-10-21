//! Case-specific vectors; navigation and native custody use the original driver.
use super::*;
use endpoint::shared_frozen as source;

pub(super) fn vectors(fetched: bool) -> Value {
    let mut rows = vec![
        json!([
            "workflow-activity-channel-trigger-1",
            endpoint::KNOWN,
            "activity-known",
            "Tag",
            crate::icons::Icon::Tag.path()
        ]),
        json!([
            "workflow-activity-channel-trigger-3",
            endpoint::KNOWN,
            "activity-known",
            "Tag",
            crate::icons::Icon::Tag.path()
        ]),
        json!([
            "workflow-activity-channel-step-0-1",
            endpoint::MISSING,
            if fetched {
                "activity-fetched"
            } else {
                "Channel unavailable"
            },
            if fetched { Some("Tag") } else { None },
            if fetched {
                Some(crate::icons::Icon::Tag.path())
            } else {
                None
            }
        ]),
    ];
    rows.extend(
        source::specs()
            .into_iter()
            .enumerate()
            .map(|(i, (id, _, label, icon))| {
                let path = match icon {
                    "LockFilled" => crate::icons::Icon::LockFilled.path(),
                    "Archive" => crate::icons::Icon::Archive.path(),
                    "ArchiveFilled" => crate::icons::Icon::ArchiveFilled.path(),
                    _ => crate::icons::Icon::Tag.path(),
                };
                json!([
                    format!("workflow-activity-channel-step-1-{}", i * 2 + 1),
                    id,
                    label,
                    icon,
                    path
                ])
            }),
    );
    json!(rows)
}
pub(super) async fn labels(desktop: &DesktopContext, fetched: bool) -> Result<Value, String> {
    let original = include_str!("special_labels.js");
    let start = original
        .find(" const vectors=")
        .ok_or("Missing vector boundary")?;
    let end = original
        .find(" const rect=")
        .ok_or("Missing measurement boundary")?;
    let script = format!(
        "{} const vectors={};\n{}",
        original[..start].replace("FETCHED", &json!(fetched).to_string()),
        vectors(fetched),
        &original[end..]
    );
    wait_dom(desktop, &script).await
}
/// Label icon and composer policy are deliberately independent.
pub(super) fn target(label: &str) -> Option<(&'static str, bool)> {
    match label {
        "frozen-active" => Some(("frozen-active", false)),
        "shared-archived" => Some(("shared-archived", true)),
        "private-grace" => Some(("grace-private", false)),
        "archived-grace" => Some(("grace-archived", true)),
        _ => None,
    }
}
pub(super) async fn archived_target(
    desktop: &DesktopContext,
    name: &str,
    timeline: &str,
) -> Result<Value, String> {
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
    let targets = if source::grace() {
        [
            ("CGRPRIV01", 1, "private-grace", "private-grace-return"),
            ("CGRARCH01", 3, "archived-grace", "archived-grace-return"),
        ]
    } else {
        [
            ("CFRZACT01", 5, "frozen-active", "frozen-active-return"),
            ("CSHARCH01", 3, "shared-archived", "shared-archived-return"),
        ]
    };
    for (id, ordinal, opened, returned) in targets {
        click(desktop, "workflow-management-activity").await?;
        detail(state, desktop).await?;
        let original_labels = labels(desktop, false).await?;
        let before = endpoint::snapshot(&state.peek());
        let button = format!("workflow-activity-channel-step-1-{ordinal}");
        click(desktop, &button).await?;
        channel_wait(state, "local shared/frozen destination", |v| {
            v["selected"] == id
        })
        .await?;
        let after = endpoint::snapshot(&state.peek());
        if before["documents"] != after["documents"] || before["ledger"] != after["ledger"] {
            return Err("Local shared/frozen navigation changed draft or dispatched a read".into());
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
