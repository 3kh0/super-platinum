//! Pure mount-time display labels; never acquisition or insertion authority.
use super::*;
use std::collections::BTreeMap;
#[cfg(test)]
mod tests;

pub(crate) fn draft_file_labels(
    shell: &ShellState,
    team: &str,
    blocks: &[Value],
) -> BTreeMap<String, String> {
    if shell.core.active_team.as_deref() != Some(team) {
        return BTreeMap::new();
    }
    let Ok(source) = file_source::Snapshot::capture(shell, team) else {
        return BTreeMap::new();
    };
    let Some(user) = shell
        .core
        .session
        .as_ref()
        .and_then(|s| s.workspaces.get(team))
        .map(|w| w.user_id.as_str())
    else {
        return BTreeMap::new();
    };
    let labels = collect(blocks, |token| {
        let id = token["file_id"].as_str()?;
        let candidate = source.rows().iter().find(|row| row.id() == id)?;
        let title = source.title(id).ok()?;
        if title.source() != candidate.source() {
            return None;
        }
        token_label(shell, team, user, token, title)
    });
    // No persistent/UI lease is created. The caller supplies labels only for
    // this mounted draft; a later mount captures the source again.
    if source.current(shell) {
        labels
    } else {
        BTreeMap::new()
    }
}

fn token_label(
    shell: &ShellState,
    team: &str,
    user: &str,
    token: &Value,
    title: file_title_source::Observation,
) -> Option<String> {
    let id = token["file_id"].as_str()?;
    let canonical = CanonicalTitle::capture(shell, &title, id).ok()?;
    let expected = if canonical.kind == file_titles::Kind::Canvas {
        "canvas"
    } else {
        "file"
    };
    if token["type"].as_str() != Some(expected) || !canonical.current_quip(shell) {
        return None;
    }
    // This validates the whole-token variant and retains its exact raw shape;
    // token text/URL and unknown metadata are never title authority.
    let mut context = RichTextContext::new_ordered(team, user).ok()?;
    context.file(token, canonical.label.clone()).ok()?;
    Some(canonical.label)
}

fn collect(
    blocks: &[Value],
    mut resolve: impl FnMut(&Value) -> Option<String>,
) -> BTreeMap<String, String> {
    let mut labels = BTreeMap::new();
    let mut pending: Vec<_> = blocks.iter().collect();
    while let Some(value) = pending.pop() {
        if matches!(value["type"].as_str(), Some("canvas" | "file")) {
            if let Some(label) = resolve(value) {
                if let Ok(key) = serde_json::to_string(value) {
                    labels.insert(key, label);
                }
            }
        }
        if let Some(elements) = value.get("elements").and_then(Value::as_array) {
            pending.extend(elements);
        }
    }
    labels
}
