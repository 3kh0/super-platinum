//! A selected result is owned by this exact picker visit, never an arbitrary ID.
use super::*;
use crate::state::ComposerTarget;
use super_platinum_core::slack::{api::HostedFileKind, models::File};

pub(crate) struct Picker {
    identity: Arc<()>,
    context: admission::Context,
    key: super_platinum_core::drafts::DraftKey,
    view: u64,
    generation: u64,
    mount: String,
}
#[derive(Clone)]
pub(crate) struct Page {
    identity: Arc<()>,
    revision: u64,
    pub items: Vec<Value>,
    pub page: u32,
    pub more: bool,
}
#[derive(Clone)]
pub(crate) struct Choice {
    identity: Arc<()>,
    revision: u64,
    raw: Value,
}
impl Page {
    pub(crate) fn choose(&self, index: usize) -> Option<Choice> {
        Some(Choice {
            identity: self.identity.clone(),
            revision: self.revision,
            raw: self.items.get(index)?.clone(),
        })
    }
}
impl Choice {
    pub(crate) fn id(&self) -> &str {
        self.raw["id"].as_str().unwrap_or_default()
    }
}
impl Picker {
    pub(crate) fn capture(shell: &ShellState) -> Result<Self, p::Error> {
        let target = ComposerTarget::Channel;
        if !shell.core.composer_attachments.is_empty() {
            return Err(p::Error::Unsupported);
        }
        let context = admission::Context::capture(shell).ok_or(p::Error::Stale)?;
        let generation = shell
            .core
            .workspaces
            .get(&context.key.team)
            .ok_or(p::Error::Stale)?
            .rt_generation;
        Ok(Self {
            identity: Arc::new(()),
            context,
            key: shell.composer_key(target).ok_or(p::Error::Stale)?,
            view: shell.draft_writes.view_id,
            generation,
            mount: crate::view::composer::field_mount_token(shell, target)
                .ok_or(p::Error::Stale)?,
        })
    }
    pub(crate) fn current(&self, shell: &ShellState) -> bool {
        self.context.current(shell)
            && shell.draft_writes.view_id == self.view
            && shell.composer_key(ComposerTarget::Channel).as_ref() == Some(&self.key)
            && shell
                .core
                .workspaces
                .get(&self.context.key.team)
                .is_some_and(|w| w.rt_generation == self.generation)
            && crate::view::composer::field_mount_token(shell, ComposerTarget::Channel).as_ref()
                == Some(&self.mount)
            && shell.core.composer_attachments.is_empty()
    }
    pub(crate) async fn search(
        &self,
        state: Signal<ShellState>,
        text: &str,
        kind: HostedFileKind,
        page: u32,
    ) -> Result<Page, p::Error> {
        if !self.current(&state.peek()) {
            return Err(p::Error::Stale);
        }
        let revision = crate::file_events::snapshot_revision(&state.peek());
        let raw = self
            .context
            .hosted_picker_search(text, kind, page)
            .await
            .map_err(|_| p::Error::Gap)?;
        if !self.current(&state.peek())
            || raw["ok"] != true
            || raw.get("error").is_some_and(|v| !v.is_null())
            || !admission::bounded(&raw, 4 * 1024 * 1024, 32)
        {
            return Err(p::Error::Stale);
        }
        let items = raw["items"]
            .as_array()
            .filter(|v| v.len() <= 50)
            .ok_or(p::Error::Unsupported)?;
        let mut ids = std::collections::BTreeSet::new();
        let mut accepted = Vec::new();
        for item in items {
            let id = item["id"].as_str().ok_or(p::Error::Unsupported)?;
            if !ids.insert(id) {
                return Err(p::Error::Unsupported);
            }
            let file: File =
                serde_json::from_value(item.clone()).map_err(|_| p::Error::Unsupported)?;
            if policy::source(&file) == Eligibility::Allowed
                && !crate::file_events::changed_since(
                    &state.peek(),
                    &self.context.key.team,
                    id,
                    revision,
                )
                && !state
                    .peek()
                    .core
                    .workspaces
                    .get(&self.context.key.team)
                    .is_none_or(|w| w.deleted_files.contains(id))
            {
                accepted.push(item.clone());
            }
        }
        let pages = raw["pagination"]["page_count"]
            .as_u64()
            .filter(|count| *count <= 1000)
            .ok_or(p::Error::Unsupported)?;
        if raw["pagination"]
            .get("page")
            .is_some_and(|v| v.as_u64() != Some(u64::from(page)))
            || (pages == 0 && !items.is_empty())
            || (pages > 0 && u64::from(page) > pages)
        {
            return Err(p::Error::Unsupported);
        }
        Ok(Page {
            identity: self.identity.clone(),
            revision,
            items: accepted,
            page,
            more: pages > u64::from(page),
        })
    }
    fn choice_current(&self, shell: &ShellState, choice: &Choice) -> bool {
        self.current(shell)
            && Arc::ptr_eq(&self.identity, &choice.identity)
            && !crate::file_events::changed_since(
                shell,
                &self.context.key.team,
                choice.id(),
                choice.revision,
            )
            && shell
                .core
                .workspaces
                .get(&self.context.key.team)
                .is_some_and(|w| !w.deleted_files.contains(choice.id()))
    }
    pub(crate) async fn insert(
        &self,
        state: Signal<ShellState>,
        choice: Choice,
    ) -> Result<(), p::Error> {
        if !self.choice_current(&state.peek(), &choice) {
            return Err(p::Error::Stale);
        }
        let response = self
            .context
            .file_info(choice.id())
            .await
            .map_err(|_| p::Error::Gap)?;
        if !self.choice_current(&state.peek(), &choice)
            || response["ok"] != true
            || response.get("error").is_some_and(|v| !v.is_null())
            || !admission::bounded(&response, 512 * 1024, 32)
        {
            return Err(p::Error::Stale);
        }
        let raw = response.get("file").cloned().ok_or(p::Error::Unsupported)?;
        let selected: File =
            serde_json::from_value(choice.raw.clone()).map_err(|_| p::Error::Unsupported)?;
        let fresh: File = serde_json::from_value(raw.clone()).map_err(|_| p::Error::Unsupported)?;
        if fresh.id.as_deref() != Some(choice.id())
            || policy::source(&fresh) != Eligibility::Allowed
            || super_platinum_core::file_actions::identity(&fresh)
                != super_platinum_core::file_actions::identity(&selected)
            || super_platinum_core::state::file_title(&fresh)
                != super_platinum_core::state::file_title(&selected)
        {
            return Err(p::Error::ReviewRequired);
        }
        crate::drafts::protected::production::hosted::edit(
            state,
            ComposerTarget::Channel,
            |composer, version| {
                if !self.choice_current(&state.peek(), &choice) {
                    return Err(p::Error::Stale);
                }
                composer.insert(version, raw)
            },
        )
        .await
    }
}
