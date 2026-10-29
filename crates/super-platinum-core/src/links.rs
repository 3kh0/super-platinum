//! Channel-scoped links from conversations.searchLinks. Pages are bounded to 20.
use crate::slack::models::SearchPagination;
use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ChannelLink {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub thumb_url: Option<String>,
    #[serde(default)]
    pub timestamp: String,
}
#[derive(Debug, Default, Deserialize)]
pub struct LinksPage {
    #[serde(default)]
    pub items: Vec<ChannelLink>,
    #[serde(default)]
    pub pagination: Option<SearchPagination>,
}
#[derive(Debug, Default)]
pub struct LinksState {
    pub scope: Option<(u64, String, String)>,
    pub generation: u64,
    pub query: String,
    pub items: Vec<ChannelLink>,
    /// The last successfully loaded page; never relabel retained rows.
    pub page: u32,
    /// Transient target retained across failure/cancellation for exact retry.
    pub requested_page: Option<u32>,
    pub page_count: u32,
    pub total: u64,
    pub loading: bool,
    pub loaded: bool,
    pub error: Option<String>,
}
impl LinksState {
    pub fn matches_scope(&self, epoch: u64, team: &str, channel: &str) -> bool {
        self.scope
            .as_ref()
            .is_some_and(|(e, t, c)| *e == epoch && t == team && c == channel)
    }
    pub fn ensure_scope(&mut self, epoch: u64, team: &str, channel: &str) {
        if !self.matches_scope(epoch, team, channel) {
            *self = Self {
                scope: Some((epoch, team.into(), channel.into())),
                generation: self.generation.wrapping_add(1),
                ..Default::default()
            };
        }
    }
    pub fn accepts(&self, generation: u64) -> bool {
        self.generation == generation && self.loading && self.requested_page.is_some()
    }
    /// Resume an unresolved target; explicit Refresh may use the loaded page.
    pub fn resume_page(&self) -> u32 {
        self.requested_page.unwrap_or(self.page).max(1)
    }
    pub fn fail(&mut self, generation: u64, error: impl Into<String>) -> bool {
        if !self.accepts(generation) {
            return false;
        }
        self.loading = false;
        self.error = Some(error.into());
        // Failure is terminal: its late response can no longer publish.
        self.generation = self.generation.wrapping_add(1);
        true
    }
    pub fn cancel(&mut self, generation: u64) -> bool {
        self.fail(generation, "Loading stopped. Try again.")
    }
    pub fn begin(&mut self, query: String, page: u32) -> u64 {
        let query = query.trim().to_owned();
        let page = page.max(1);
        if self.query != query {
            self.items.clear();
            self.total = 0;
            self.page_count = 0;
            self.loaded = false;
        }
        if !self.loaded {
            self.page = 1;
        }
        self.query = query;
        self.requested_page = Some(page);
        self.loading = true;
        self.error = None;
        self.generation = self.generation.wrapping_add(1);
        self.generation
    }
    pub fn apply(&mut self, generation: u64, response: LinksPage) -> bool {
        if !self.accepts(generation) {
            return false;
        }
        let requested_page = self.requested_page.take().expect("accepted page target");
        let pagination = response.pagination.unwrap_or_default();
        self.page = pagination.page.unwrap_or(requested_page).max(1);
        self.page_count = pagination.page_count.unwrap_or(1).max(1);
        self.total = pagination
            .total_count
            .unwrap_or(response.items.len() as u64);
        let mut keys = std::collections::HashSet::new();
        self.items = response
            .items
            .into_iter()
            .filter(|item| {
                !item.url.is_empty() && keys.insert((item.url.clone(), item.timestamp.clone()))
            })
            .take(20)
            .collect();
        self.loading = false;
        self.loaded = true;
        self.error = None;
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn channel_account_search_and_page_transitions_reject_late_results() {
        let mut state = LinksState::default();
        state.ensure_scope(1, "T1", "C1");
        let old = state.begin("old".into(), 1);
        let search = state.begin("new".into(), 1);
        assert!(!state.apply(old, LinksPage::default()));
        let next = state.begin("new".into(), 2);
        assert!(!state.apply(search, LinksPage::default()));
        state.ensure_scope(1, "T1", "C2");
        assert!(!state.apply(next, LinksPage::default()));
        let old_account = state.begin("new".into(), 1);
        state.ensure_scope(2, "T1", "C2");
        assert!(!state.apply(old_account, LinksPage::default()));
        assert!(!state.matches_scope(1, "T1", "C2"));
    }
    #[test]
    fn page_deduplicates_same_message_but_keeps_repeated_links_in_other_messages() {
        let mut state = LinksState::default();
        let generation = state.begin(String::new(), 1);
        let page = serde_json::from_value(serde_json::json!({"items":[
            {"url":"https://example.com","title":"Hello 🚀","timestamp":"1.0"},
            {"url":"https://example.com","timestamp":"1.0"},
            {"url":"https://example.com","title":null,"icon_url":null,"thumb_url":null,"timestamp":"2.0"}
        ],"pagination":{"total_count":104,"page_count":6,"page":1}}))
        .unwrap();
        assert!(state.apply(generation, page));
        assert_eq!(state.items.len(), 2);
        assert_eq!(state.items[0].title.as_deref(), Some("Hello 🚀"));
        assert_eq!(state.total, 104);
        assert_eq!(state.page_count, 6);
    }
}

#[cfg(test)]
mod resource_page_recovery;
