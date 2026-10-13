//! Search request identity and paging, independent of the renderer.
use crate::slack::models::SearchPagination;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SearchSort {
    Relevant,
    #[default]
    Newest,
    Oldest,
}
impl SearchSort {
    pub const ALL: [Self; 3] = [Self::Relevant, Self::Newest, Self::Oldest];
    pub fn label(self) -> &'static str {
        match self {
            Self::Relevant => "Most relevant",
            Self::Newest => "Newest first",
            Self::Oldest => "Oldest first",
        }
    }
    pub fn api(self) -> (&'static str, &'static str) {
        match self {
            Self::Relevant => ("score", "desc"),
            Self::Newest => ("timestamp", "desc"),
            Self::Oldest => ("timestamp", "asc"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchRequest {
    pub epoch: u64,
    pub team: String,
    pub query: String,
    pub sort: SearchSort,
    pub page: u32,
    sequence: u64,
}

#[derive(Debug, Clone, Default)]
pub struct SearchPager {
    pub query: String,
    pub sort: SearchSort,
    pub page: u32,
    pub page_count: u32,
    pub total: u64,
    pub loading: bool,
    pub failed: bool,
    epoch: u64,
    team: String,
    sequence: u64,
}

impl SearchPager {
    pub fn revision(&self) -> u64 {
        self.sequence
    }

    pub fn reset(&mut self) {
        let sequence = self.sequence.wrapping_add(1);
        *self = Self {
            sort: self.sort,
            sequence,
            ..Self::default()
        };
    }

    pub fn change_sort(&mut self, sort: SearchSort) -> bool {
        if self.sort == sort {
            return false;
        }
        self.reset();
        self.sort = sort;
        true
    }

    pub fn has_more(&self) -> bool {
        self.page > 0 && self.page < self.page_count
    }

    pub fn begin(
        &mut self,
        epoch: u64,
        team: &str,
        query: &str,
        append: bool,
    ) -> Option<SearchRequest> {
        let query = query.trim();
        if query.is_empty() {
            self.reset();
            return None;
        }
        let same = self.epoch == epoch && self.team == team && self.query == query;
        if append && (!same || self.loading || !self.has_more()) {
            return None;
        }
        if same && self.loading {
            return None;
        }
        if !append {
            self.reset();
            self.epoch = epoch;
            self.team = team.into();
            self.query = query.into();
        }
        self.sequence = self.sequence.wrapping_add(1);
        self.loading = true;
        self.failed = false;
        Some(SearchRequest {
            epoch,
            team: team.into(),
            query: query.into(),
            sort: self.sort,
            page: self.page + 1,
            sequence: self.sequence,
        })
    }

    pub fn accepts(&self, request: &SearchRequest) -> bool {
        self.loading
            && self.sequence == request.sequence
            && self.epoch == request.epoch
            && self.team == request.team
            && self.query == request.query
            && self.sort == request.sort
    }

    pub fn fail(&mut self, request: &SearchRequest) -> bool {
        if !self.accepts(request) {
            return false;
        }
        self.loading = false;
        self.failed = true;
        true
    }

    pub fn complete(
        &mut self,
        request: &SearchRequest,
        pagination: Option<&SearchPagination>,
    ) -> bool {
        if !self.accepts(request) {
            return false;
        }
        // A mismatched page must not advance the cursor or replace visible results.
        if pagination
            .and_then(|p| p.page)
            .is_some_and(|page| page != request.page)
        {
            self.fail(request);
            return false;
        }
        self.page = request.page;
        self.page_count = pagination
            .and_then(|p| p.page_count)
            .unwrap_or(self.page)
            .max(self.page);
        self.total = pagination.and_then(|p| p.total_count).unwrap_or(0);
        self.loading = false;
        self.failed = false;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn page(n: u32) -> SearchPagination {
        SearchPagination {
            page: Some(n),
            page_count: Some(3),
            total_count: Some(57),
            ..Default::default()
        }
    }
    #[test]
    fn sorting_invalidates_pending_pages_and_survives_query_edits() {
        let mut s = SearchPager::default();
        let old = s.begin(1, "T", "a", false).unwrap();
        assert!(s.change_sort(SearchSort::Oldest));
        assert!(!s.accepts(&old));
        let new = s.begin(1, "T", "a", false).unwrap();
        assert_eq!(new.page, 1);
        assert_eq!(new.sort, SearchSort::Oldest);
        s.reset();
        assert_eq!(s.sort, SearchSort::Oldest);
        assert!(!s.change_sort(SearchSort::Oldest));
    }
    #[test]
    fn ignores_late_success_and_failure_after_query_aba() {
        let mut s = SearchPager::default();
        let old = s.begin(1, "T", "a", false).unwrap();
        s.begin(1, "T", "b", false).unwrap();
        let new = s.begin(1, "T", "a", false).unwrap();
        assert!(!s.complete(&old, Some(&page(1))));
        assert!(!s.fail(&old));
        assert!(s.accepts(&new));
    }
    #[test]
    fn ignores_replaced_accounts_workspaces_and_cleared_inputs() {
        let mut s = SearchPager::default();
        let a = s.begin(1, "T", "a", false).unwrap();
        let b = s.begin(1, "OTHER", "a", false).unwrap();
        let c = s.begin(2, "T", "a", false).unwrap();
        assert!(!s.accepts(&a));
        assert!(!s.accepts(&b));
        assert!(s.accepts(&c));
        s.reset();
        assert!(!s.complete(&c, Some(&page(1))));
        assert!(!s.loading);
    }
    #[test]
    fn pages_once_and_retries_same_failed_page() {
        let mut s = SearchPager::default();
        let a = s.begin(1, "T", "a", false).unwrap();
        assert!(s.begin(1, "T", "a", false).is_none());
        assert!(s.complete(&a, Some(&page(1))));
        let b = s.begin(1, "T", "a", true).unwrap();
        assert_eq!(b.page, 2);
        assert!(s.begin(1, "T", "a", true).is_none());
        assert!(s.fail(&b));
        assert_eq!(s.total, 57);
        assert_eq!(s.page, 1);
        let retry = s.begin(1, "T", "a", true).unwrap();
        assert_eq!(retry.page, 2);
        assert!(s.complete(&retry, Some(&page(2))));
        let last = s.begin(1, "T", "a", true).unwrap();
        s.complete(&last, Some(&page(3)));
        assert!(!s.has_more());
        assert!(s.begin(1, "T", "a", true).is_none());
    }
    #[test]
    fn rejects_wrong_pages_and_missing_metadata_stops_paging() {
        let mut s = SearchPager::default();
        let a = s.begin(1, "T", "a", false).unwrap();
        assert!(!s.complete(&a, Some(&page(2))));
        assert!(s.failed);
        assert_eq!(s.page, 0);
        let b = s.begin(1, "T", "a", false).unwrap();
        assert!(s.complete(&b, None));
        assert!(!s.has_more());
    }
    #[test]
    fn changing_query_cannot_append_to_previous_results() {
        let mut s = SearchPager::default();
        let a = s.begin(1, "T", "a", false).unwrap();
        s.complete(&a, Some(&page(1)));
        assert!(s.begin(1, "T", "b", true).is_none());
        let b = s.begin(1, "T", "b", false).unwrap();
        assert_eq!(b.page, 1);
        assert_eq!(s.total, 0);
    }
}
