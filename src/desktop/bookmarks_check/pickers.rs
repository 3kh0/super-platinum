//! Only reached by the disconnected bookmark check. No mutation/transport seam.
use super::*;
use serde_json::{Value, json};
pub(super) const STAGES: [&str; 8] = [
    "prepopulated",
    "child-initial",
    "child-reopened",
    "child-before",
    "child-reset",
    "changed",
    "offline-retained",
    "reset",
];
pub(super) fn seed(mut state: Signal<ShellState>) -> Result<(), String> {
    let mut shell = state.write();
    let store = &mut shell.core.bookmarks;
    if store.draft.is_some() {
        return Err("Unexpected initial bookmark draft".into());
    }
    let base = store
        .items
        .iter()
        .find(|b| b.id == "Bk1")
        .cloned()
        .ok_or("Missing fixture bookmark")?;
    let mut draft = super_platinum_core::bookmarks::Draft::new(Some(base), false);
    draft.movement = Some(super_platinum_core::bookmarks::Movement {
        parent: "Bk2".into(),
        before: "Bk3".into(),
    });
    store.draft = Some(draft);
    Ok(())
}
pub(super) async fn capture(
    state: Signal<ShellState>,
    index: usize,
    dom: Value,
) -> Result<Value, String> {
    let stage = STAGES.get(index).ok_or("Extra picker stage")?;
    if dom["stage"] != *stage {
        return Err("Wrong picker stage order".into());
    }
    let canonical = {
        let shell = state.peek();
        if shell.core.session.is_some()
            || shell.core.transport.is_some()
            || shell.core.active_team.as_deref() != Some("T1")
            || shell.core.active_channel.as_deref() != Some("C2")
        {
            return Err("Changed disconnected scope".into());
        }
        let d = shell
            .core
            .bookmarks
            .draft
            .as_ref()
            .ok_or("Missing movement draft")?;
        let m = d.movement.as_ref().ok_or("Missing movement")?;
        json!({"base":d.base.as_ref().ok_or("Missing base")?.id,"parent":m.parent,"before":m.before})
    };
    if dom["parent"]["id"] != canonical["parent"] || dom["before"]["id"] != canonical["before"] {
        return Err("DOM picker differs from canonical movement".into());
    }
    let fixture = std::env::var("SUPER_PLATINUM_FIXTURE").map_err(|_| "Missing fixture")?;
    if !matches!(fixture.as_str(), "bookmarks" | "bookmarks-narrow") {
        return Err("Unsupported picker fixture".into());
    }
    let Some(directory) = std::env::var_os("SUPER_PLATINUM_BOOKMARKS_CAPTURE_DIR") else {
        return Ok(json!({"dom":dom,"canonical":canonical,"capture":null}));
    };
    let dir = std::path::PathBuf::from(directory);
    if !dir.is_absolute() {
        return Err("Absolute capture directory required".into());
    }
    let path = dir.join(format!("{fixture}-{stage}.png"));
    let desktop = dioxus::desktop::window();
    let (path, width, height) =
        crate::agent::capture::window(Some(path.to_string_lossy().into_owned()), &desktop).await?;
    Ok(
        json!({"dom":dom,"canonical":canonical,"capture":{"path":path.to_string_lossy(),"width":width,"height":height}}),
    )
}
