//! Account-scoped Files browser queries and bounded search results.
use crate::slack::models::{File, SearchPagination};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FileKind {
    #[default]
    All,
    Documents,
    Canvases,
    Lists,
    Images,
    Videos,
    Audio,
    Pdfs,
    Spreadsheets,
    Presentations,
    Snippets,
}
impl FileKind {
    pub const ALL: [Self; 11] = [
        Self::All,
        Self::Documents,
        Self::Canvases,
        Self::Lists,
        Self::Images,
        Self::Videos,
        Self::Audio,
        Self::Pdfs,
        Self::Spreadsheets,
        Self::Presentations,
        Self::Snippets,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All types",
            Self::Documents => "Documents",
            Self::Canvases => "Canvases",
            Self::Lists => "Lists",
            Self::Images => "Images",
            Self::Videos => "Videos",
            Self::Audio => "Audio",
            Self::Pdfs => "PDFs",
            Self::Spreadsheets => "Spreadsheets",
            Self::Presentations => "Presentations",
            Self::Snippets => "Snippets",
        }
    }
    pub fn query(self) -> &'static str {
        match self {
            Self::All => "",
            Self::Documents => "type:documents",
            Self::Canvases => "type:quip",
            Self::Lists => "type:lists",
            Self::Images => "type:images",
            Self::Videos => "type:videos",
            Self::Audio => "type:audio",
            Self::Pdfs => "type:pdfs",
            Self::Spreadsheets => "type:spreadsheets",
            Self::Presentations => "type:presentations",
            Self::Snippets => "type:snippets",
        }
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FileOwner {
    #[default]
    All,
    Yours,
    Shared,
}
impl FileOwner {
    pub const ALL: [Self; 3] = [Self::All, Self::Yours, Self::Shared];
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All files",
            Self::Yours => "Created by you",
            Self::Shared => "Shared with you",
        }
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FileSort {
    #[default]
    Viewed,
    Newest,
    Relevant,
}
impl FileSort {
    pub const ALL: [Self; 3] = [Self::Viewed, Self::Newest, Self::Relevant];
    pub fn label(self) -> &'static str {
        match self {
            Self::Viewed => "Recently viewed",
            Self::Newest => "Newest first",
            Self::Relevant => "Most relevant",
        }
    }
    pub fn api(self) -> &'static str {
        match self {
            Self::Viewed => "last_engaged",
            Self::Newest => "timestamp",
            Self::Relevant => "score",
        }
    }
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FilesQuery {
    pub text: String,
    /// A channel resource browser keeps its scope separate from global search.
    pub channel: Option<String>,
    pub kind: FileKind,
    pub owner: FileOwner,
    pub sort: FileSort,
}
impl FilesQuery {
    /// One accepted identity for state comparison and the issued request.
    pub fn normalized(mut self) -> Self {
        self.text = self.text.trim().to_owned();
        self.sort = self.effective_sort();
        self
    }

    /// Slack's engagement index ignores text/channel/creator filters. Use the
    /// actual search index whenever one of those filters is active.
    pub fn effective_sort(&self) -> FileSort {
        if self.sort == FileSort::Viewed {
            if !self.text.trim().is_empty()
                || self.owner == FileOwner::Shared
                || self.channel.is_some()
            {
                return FileSort::Relevant;
            }
            if self.owner == FileOwner::Yours {
                return FileSort::Newest;
            }
        }
        self.sort
    }
    pub fn search_text(&self, user: &str) -> String {
        let mut parts = vec![self.text.trim().to_owned(), self.kind.query().to_owned()];
        if let Some(channel) = &self.channel {
            parts.push(format!("in:<#{channel}>"));
        }
        if self.owner == FileOwner::Yours {
            // `creator:` is the canvas/list index; ordinary uploaded files use
            // `from:`. Applying `creator:` to an image hides even our uploads.
            let modifier = if matches!(self.kind, FileKind::Canvases | FileKind::Lists) {
                "creator"
            } else {
                "from"
            };
            parts.push(format!("{modifier}:<@{user}>"));
        }
        let query = parts
            .into_iter()
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if query.is_empty() { "*".into() } else { query }
    }
}
#[derive(Debug, Default, Deserialize)]
pub struct FilesPage {
    #[serde(default)]
    pub items: Vec<File>,
    #[serde(default)]
    pub pagination: Option<SearchPagination>,
}
#[derive(Debug, Default)]
pub struct FilesState {
    /// Search indexing can lag deletion. Keep removed IDs across queries in this scope.
    pub deleted: std::collections::HashSet<String>,
    pub scope: Option<(u64, String)>,
    pub generation: u64,
    pub query: FilesQuery,
    pub items: Vec<File>,
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
impl FilesState {
    pub fn remove(&mut self, id: &str) {
        if id.is_empty() {
            return;
        }
        self.deleted.insert(id.into());
        let before = self.items.len();
        self.items.retain(|f| f.id.as_deref() != Some(id));
        self.total = self
            .total
            .saturating_sub((before - self.items.len()) as u64);
    }
    pub fn matches_scope(&self, epoch: u64, team: &str) -> bool {
        self.scope
            .as_ref()
            .is_some_and(|(e, t)| *e == epoch && t == team)
    }
    pub fn ensure_scope(&mut self, epoch: u64, team: &str) {
        if !self.matches_scope(epoch, team) {
            *self = Self {
                scope: Some((epoch, team.into())),
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
    pub fn begin(&mut self, query: FilesQuery, page: u32) -> u64 {
        let query = query.normalized();
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
    pub fn apply(&mut self, generation: u64, response: FilesPage) -> bool {
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
        self.total = self.total.saturating_sub(
            response
                .items
                .iter()
                .filter(|f| f.id.as_ref().is_some_and(|id| self.deleted.contains(id)))
                .count() as u64,
        );
        let mut ids = std::collections::HashSet::new();
        self.items = response
            .items
            .into_iter()
            .filter(|f| !f.id.as_ref().is_some_and(|id| self.deleted.contains(id)))
            .filter(|f| f.id.as_ref().is_some_and(|id| ids.insert(id.clone())))
            .take(50)
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
    fn filter_queries_preserve_search_syntax_and_current_user() {
        let q = FilesQuery {
            text: "  in:private plan 🚀  ".into(),
            kind: FileKind::Pdfs,
            owner: FileOwner::Yours,
            ..Default::default()
        };
        assert_eq!(
            q.search_text("U1"),
            "in:private plan 🚀 type:pdfs from:<@U1>"
        );
        assert_eq!(FilesQuery::default().search_text("U1"), "*");
        assert_eq!(
            FilesQuery {
                kind: FileKind::Canvases,
                owner: FileOwner::Yours,
                ..Default::default()
            }
            .search_text("U1"),
            "type:quip creator:<@U1>"
        );
        assert_eq!(q.effective_sort(), FileSort::Relevant);
        assert_eq!(
            FilesQuery {
                owner: FileOwner::Yours,
                ..Default::default()
            }
            .effective_sort(),
            FileSort::Newest
        );
        assert_eq!(FilesQuery::default().effective_sort(), FileSort::Viewed);
    }
    #[test]
    fn old_query_and_account_responses_cannot_replace_current_results() {
        let mut s = FilesState::default();
        s.ensure_scope(1, "T1");
        let old = s.begin(Default::default(), 1);
        let current = s.begin(
            FilesQuery {
                kind: FileKind::Images,
                ..Default::default()
            },
            1,
        );
        assert!(!s.apply(old, FilesPage::default()));
        let page=serde_json::from_value(serde_json::json!({"items":[{"id":"F1","title":"one"},{"id":"F1","title":"duplicate"}],"pagination":{"page":1,"page_count":2,"total_count":70}})).unwrap();
        assert!(s.apply(current, page));
        assert_eq!(s.items.len(), 1);
        assert_eq!(s.total, 70);
        assert_eq!(s.page_count, 2);
        s.ensure_scope(2, "T2");
        assert!(s.items.is_empty());
        assert!(!s.apply(current, FilesPage::default()));
    }
}

#[cfg(test)]
mod resource_page_recovery;
