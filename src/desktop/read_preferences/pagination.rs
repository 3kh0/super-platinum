//! Resumable history pagination; page budgets never restart a completed prefix.
use std::collections::HashSet;
use super_platinum_core::{
    slack::{Error, models::HistoryPage},
    state::cmp_ts,
};
#[derive(Clone, Default)]
pub(crate) struct Pages {
    pub cursor: Option<String>,
    pub latest: Option<String>,
    seen: HashSet<String>,
}
impl Pages {
    pub fn advance(&mut self, page: &HistoryPage, boundary: &str) -> Result<bool, Error> {
        let next = page
            .response_metadata
            .as_ref()
            .and_then(|meta| meta.next_cursor.as_ref())
            .filter(|s| !s.is_empty());
        if let Some(cursor) = next {
            if !self.seen.insert(cursor.clone()) {
                return Err(invalid());
            }
            self.cursor = Some(cursor.clone());
            return Ok(true);
        }
        if !page.has_more {
            return Ok(false);
        }
        // Slack documents using the oldest returned timestamp as the next
        // exclusive latest boundary when a time range still has more messages.
        let oldest = page
            .messages
            .iter()
            .filter_map(|m| m.ts.as_deref())
            .min_by(|a, b| cmp_ts(Some(a), Some(b)))
            .ok_or_else(invalid)?;
        if !cmp_ts(Some(oldest), Some(boundary)).is_gt()
            || self
                .latest
                .as_deref()
                .is_some_and(|previous| !cmp_ts(Some(oldest), Some(previous)).is_lt())
        {
            return Err(invalid());
        }
        self.cursor = None;
        self.latest = Some(oldest.into());
        Ok(true)
    }
}
fn invalid() -> Error {
    Error::Api("history_opening_did_not_advance".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn page(value: serde_json::Value) -> HistoryPage {
        serde_json::from_value(value).unwrap()
    }
    #[test]
    fn paused_chunks_resume_after_the_completed_prefix_and_retain_loop_protection() {
        let mut pages = Pages::default();
        for index in 1..=8 {
            assert!(
                pages
                    .advance(
                        &page(json!({"response_metadata":{"next_cursor":format!("page-{index}")}})),
                        "10.000001"
                    )
                    .unwrap()
            );
        }
        let mut resumed = pages.clone();
        assert_eq!(resumed.cursor.as_deref(), Some("page-8"));
        assert!(
            resumed
                .advance(
                    &page(json!({"response_metadata":{"next_cursor":"page-9"}})),
                    "10.000001"
                )
                .unwrap()
        );
        assert!(
            resumed
                .advance(
                    &page(json!({"response_metadata":{"next_cursor":"page-2"}})),
                    "10.000001"
                )
                .is_err()
        );
    }
    #[test]
    fn short_pages_follow_cursor_and_repeated_cursor_cannot_loop() {
        let mut pages = Pages::default();
        let first = page(
            json!({"messages":[],"has_more":false,"response_metadata":{"next_cursor":"next"}}),
        );
        assert_eq!(pages.advance(&first, "10.000001").unwrap(), true);
        assert_eq!(pages.cursor.as_deref(), Some("next"));
        assert!(pages.advance(&first, "10.000001").is_err());
    }
    #[test]
    fn legacy_time_pages_use_the_oldest_timestamp_exclusively_and_require_progress() {
        let mut pages = Pages::default();
        let first =
            page(json!({"messages":[{"ts":"30.000001"},{"ts":"20.000001"}],"has_more":true}));
        assert!(pages.advance(&first, "10.000001").unwrap());
        assert_eq!(pages.latest.as_deref(), Some("20.000001"));
        assert!(pages.advance(&first, "10.000001").is_err());
        assert!(
            !pages
                .advance(
                    &page(json!({"messages":[{"ts":"11.000001"}],"has_more":false})),
                    "10.000001"
                )
                .unwrap()
        );
    }
    #[test]
    fn empty_unfinished_page_cannot_claim_the_first_unread_was_found() {
        assert!(
            Pages::default()
                .advance(&page(json!({"messages":[],"has_more":true})), "10.000001")
                .is_err()
        );
    }
}
