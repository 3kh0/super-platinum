//! Exact Messages-page admission beside the unchanged append cursor.
use super::{SearchKind, SearchPager, SearchRequest};

impl SearchPager {
    /// The caller owns the accepted server bounds and any cached page data.
    /// This only reserves a fresh request identity; it does not certify a cache
    /// or infer pagination from the number of rendered messages.
    pub fn begin_message_page(
        &mut self,
        epoch: u64,
        team: &str,
        query: &str,
        page: u32,
    ) -> Option<SearchRequest> {
        if self.kind != SearchKind::Messages
            || self.loading
            || self.epoch != epoch
            || self.team != team
            || query.trim().is_empty()
            || self.query != query.trim()
            || self.page == 0
            || page == 0
            || page > self.page_count
        {
            return None;
        }
        self.sequence = self.sequence.wrapping_add(1);
        self.loading = true;
        self.failed = false;
        Some(SearchRequest {
            kind: self.kind,
            epoch,
            team: team.into(),
            query: self.query.clone(),
            sort: self.sort,
            page,
            sequence: self.sequence,
        })
    }
}

#[cfg(test)]
mod tests;
