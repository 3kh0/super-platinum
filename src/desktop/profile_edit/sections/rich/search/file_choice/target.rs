//! Current canonical whole-file identity for a separately guarded native route.
//! Capturing this handle performs no acquisition and grants no route authority.
use super::*;
use super_platinum_core::slack::models::File;

#[cfg(test)]
mod tests;

pub(crate) struct FileTarget {
    team: String,
    raw: Value,
    source: Arc<file_source::Snapshot>,
    _title: file_title_source::Observation,
    canonical: CanonicalTitle,
    file: File,
    display: Option<Arc<FileTitles>>,
}

impl FileTarget {
    pub(crate) fn capture(shell: &ShellState, team: &str, raw: &Value) -> Result<Self, Error> {
        Self::capture_with_titles(shell, team, raw, None)
    }
    pub(crate) fn capture_with_titles(
        shell: &ShellState,
        team: &str,
        raw: &Value,
        display: Option<Arc<FileTitles>>,
    ) -> Result<Self, Error> {
        if shell.core.active_team.as_deref() != Some(team) {
            return Err(unavailable());
        }
        let id = raw["file_id"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(unavailable)?;
        // Navigation supports only the whole object. Opaque fields remain in
        // raw; active subobject/skill selectors must not silently open its parent.
        if ["section_id", "record_id", "is_skill_invocation"]
            .iter()
            .any(|key| truth(&raw[*key]))
        {
            return Err(unavailable());
        }
        let source = file_source::Snapshot::capture(shell, team)?;
        let candidate = source
            .rows()
            .iter()
            .find(|row| row.id() == id)
            .ok_or_else(unavailable)?;
        let title = source.title(id)?;
        if title.source() != candidate.source() {
            return Err(unavailable());
        }
        let canonical = match &display {
            Some(t) => t.target(shell, team, raw)?,
            None => CanonicalTitle::capture(shell, &title, id)?,
        };
        let expected = if canonical.kind == file_titles::Kind::Canvas {
            "canvas"
        } else {
            "file"
        };
        if raw["type"].as_str() != Some(expected) {
            return Err(unavailable());
        }
        let user = shell
            .core
            .session
            .as_ref()
            .and_then(|session| session.workspaces.get(team))
            .ok_or_else(unavailable)?;
        let mut context = RichTextContext::new_ordered(team, &user.user_id)?;
        context.file(raw, canonical.label.clone())?;
        // Decode exclusively from the admitted effective source. Token URL,
        // text, title, and embedded file objects are never native target facts.
        let file: File =
            serde_json::from_value(candidate.effective().clone()).map_err(|_| unavailable())?;
        if file.id.as_deref() != Some(id) {
            return Err(unavailable());
        }
        let target = Self {
            team: team.into(),
            raw: raw.clone(),
            source,
            _title: title,
            canonical,
            file,
            display,
        };
        target
            .current(shell)
            .then_some(target)
            .ok_or_else(unavailable)
    }

    /// Caller must also retain its original click/visit authority and recheck
    /// this original handle after awaits, immediately before route commitment.
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        shell.core.active_team.as_deref() == Some(self.team.as_str())
            && self.source.current(shell)
            && self.canonical.current_quip(shell)
            && self.display.as_ref().is_none_or(|t| t.current(shell))
    }

    /// Inert canonical payload; returning it does not revalidate a route.
    pub(crate) fn label(&self) -> &str {
        &self.canonical.label
    }
    pub(crate) fn file(&self) -> File {
        self.file.clone()
    }
    pub(crate) fn raw(&self) -> &Value {
        &self.raw
    }
}
