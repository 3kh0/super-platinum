//! Reply-only election. Missing metadata is a read dependency, never a title.
use super::*;

impl FileTitles {
    /// Preserve opaque/unsupported variants. Only the original closed whole
    /// Canvas grammar can ask for missing metadata before the first reply mount.
    pub(crate) fn reply_tokens(shell: &ShellState, team: &str, blocks: &[Value]) -> Vec<Value> {
        let source = file_source::Snapshot::capture(shell, team).ok();
        Self::tokens(blocks)
            .into_iter()
            .filter(|token| {
                if !whole_canvas(shell, team, token) {
                    return false;
                }
                let id = token["file_id"].as_str().unwrap();
                source.as_ref().is_none_or(|source| match source.title(id) {
                    Ok(title) => supported(&title),
                    Err(_) => !source.excluded_ids().iter().any(|v| v == id),
                })
            })
            .collect()
    }
    /// Positive current category evidence only, not an inferred absence or API error.
    pub(crate) fn reply_excluded(shell: &ShellState, team: &str, id: &str) -> bool {
        file_source::Snapshot::capture(shell, team).is_ok_and(|source| {
            source.current(shell) && source.excluded_ids().iter().any(|v| v == id)
        })
    }
    pub(crate) fn capture_reply(
        shell: &ShellState,
        team: &str,
        tokens: Vec<Value>,
        owner: Arc<dyn Fn(&ShellState) -> bool + Send + Sync>,
    ) -> Result<Self, Error> {
        if !owner(shell) {
            return Err(unavailable());
        }
        let source = file_source::Snapshot::capture(shell, team)?;
        let mut admitted = Vec::new();
        let mut excluded = Vec::new();
        for token in tokens {
            if !whole_canvas(shell, team, &token) {
                return Err(unavailable());
            }
            let id = token["file_id"].as_str().unwrap();
            match source.title(id) {
                Ok(title) if supported(&title) => admitted.push(token),
                Ok(_) => excluded.push(token),
                Err(_) if source.excluded_ids().iter().any(|v| v == id) => excluded.push(token),
                Err(error) => return Err(error),
            }
        }
        if !source.current(shell) {
            return Err(unavailable());
        }
        let mut value = Self::capture_with_context(
            shell,
            team,
            admitted,
            owner,
            Some(super::super::super::super::title_locale::CanvasTitle::english()),
        )?;
        // Same synchronous original source, including excluded receipt history.
        value.source = source;
        value.excluded_tokens = excluded;
        Ok(value)
    }
}
fn whole_canvas(shell: &ShellState, team: &str, token: &Value) -> bool {
    if token["type"] != "canvas"
        || ["section_id", "record_id", "is_skill_invocation"]
            .iter()
            .any(|key| truth(&token[*key]))
        || !token["file_id"].as_str().is_some_and(|id| {
            (2..=255).contains(&id.len())
                && id.starts_with('F')
                && id.bytes().all(|b| b.is_ascii_alphanumeric())
        })
    {
        return false;
    }
    let Some(user) = shell
        .core
        .session
        .as_ref()
        .and_then(|s| s.workspaces.get(team))
        .map(|s| &s.user_id)
    else {
        return false;
    };
    RichTextContext::new_ordered(team, user)
        .and_then(|mut c| c.file(token, "File".into()).map(|_| ()))
        .is_ok()
}
fn supported(title: &file_title_source::Observation) -> bool {
    kind(title.effective()).is_ok_and(|k| k == file_titles::Kind::Canvas)
        && file_titles::user_ids(title.effective()).is_ok()
}
