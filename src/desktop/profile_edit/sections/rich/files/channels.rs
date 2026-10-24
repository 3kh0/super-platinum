//! Known ordinary nested title channels share canonical source and hit ownership.
use super::*;
use std::collections::BTreeSet;

#[derive(Clone)]
pub(super) struct Channels {
    source: Arc<canonical::Canonical>,
    ids: BTreeSet<String>,
    presentation: super::super::title_locale::ChannelTitle,
    selected: BTreeMap<String, Arc<crate::app_home::channel_fetches::unknown::Selection>>,
}
impl Channels {
    pub(super) fn capture(
        shell: &ShellState,
        token: &Token,
        ids: &BTreeSet<String>,
    ) -> Result<Option<Self>, Error> {
        if ids.is_empty() {
            return Ok(None);
        }
        if !token.current(shell) {
            return Err(unavailable());
        }
        let source = canonical::observed(shell, &token.owner.team)?;
        let presentation = super::super::title_locale::ChannelTitle::english();
        let mut selected = BTreeMap::new();
        for id in ids {
            if source.row(id).is_some_and(|row| {
                file_titles::known_ordinary_channel_label_with_context(
                    id,
                    row,
                    Some(presentation.context()),
                )
                .is_ok()
            }) {
                continue;
            }
            let observation =
                crate::app_home::channel_fetches::unknown::observed(shell, &token.owner.team, id)
                    .ok_or_else(unavailable)?;
            file_titles::observed_channel_label(
                id,
                observation.lookup(),
                Some(presentation.observed_context()),
            )
            .map_err(|_| unavailable())?;
            selected.insert(id.clone(), observation);
        }
        Ok(Some(Self {
            source,
            ids: ids.clone(),
            presentation,
            selected,
        }))
    }
    pub(super) fn current(&self, shell: &ShellState, token: &Token) -> bool {
        token.current(shell)
            && self.source.current_for(shell, &token.owner.team)
            && self.selected.values().all(|s| s.current(shell))
    }
    pub(super) fn label(&self, id: &str) -> Result<String, file_titles::Unavailable> {
        if !self.ids.contains(id) {
            return Err(file_titles::Unavailable::InvalidMetadata);
        }
        if let Some(selected) = self.selected.get(id) {
            return file_titles::observed_channel_label(
                id,
                selected.lookup(),
                Some(self.presentation.observed_context()),
            );
        }
        file_titles::known_ordinary_channel_label_with_context(
            id,
            self.source
                .row(id)
                .ok_or(file_titles::Unavailable::InvalidMetadata)?,
            Some(self.presentation.context()),
        )
    }
    pub(super) fn ids(&self) -> impl Iterator<Item = &String> {
        self.ids.iter()
    }
    pub(super) fn snapshot(&self) -> Value {
        let mut value = serde_json::json!({"ids":self.ids,"rows":self.ids.iter().filter_map(|id|self.source.row(id)).collect::<Vec<_>>(),"presentation":self.presentation.descriptor()});
        if !self.selected.is_empty() {
            value["observed"] = Value::Array(self.selected.iter().map(|(id,s)|serde_json::json!({
                "id":id,"kind":format!("{:?}",s.lookup().kind()),"transient":s.lookup().is_transient(),"provenance":"sealed-shared-channel-selector"
            })).collect());
            value["observedPresentation"] = self.presentation.observed_descriptor();
        }
        value
    }
}
