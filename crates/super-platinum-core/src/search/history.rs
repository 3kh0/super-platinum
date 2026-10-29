//! Bounded local recent searches. No remote history or authorization evidence.
use super::{SearchKind, SearchSort};

const MAX_ENTRIES: usize = 20;
const MAX_QUERY_BYTES: usize = 4096;
const MAX_INSPECTED: usize = 80;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RecentSearch {
    pub query: String,
    pub kind: SearchKind,
    pub sort: SearchSort,
}
impl RecentSearch {
    fn valid(&self) -> bool {
        !self.query.is_empty()
            && self.query.len() <= MAX_QUERY_BYTES
            && self.query.trim() == self.query
    }
    fn same_query(&self, other: &Self) -> bool {
        self.query == other.query && self.kind == other.kind
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
#[serde(transparent)]
pub struct SearchHistory(Vec<RecentSearch>);
impl SearchHistory {
    pub fn entries(&self) -> &[RecentSearch] {
        &self.0
    }

    /// Record an admitted replacement request; never truncate search syntax.
    /// Oversized queries still search normally but are not retained here.
    pub fn record(&mut self, query: &str, kind: SearchKind, sort: SearchSort) -> bool {
        let entry = RecentSearch {
            query: query.trim().into(),
            kind,
            sort,
        };
        if !entry.valid() || self.0.first() == Some(&entry) {
            return false;
        }
        self.0.retain(|old| !old.same_query(&entry));
        self.0.insert(0, entry);
        self.0.truncate(MAX_ENTRIES);
        true
    }

    /// Decode this optional cache field independently of unrelated preferences.
    /// Unknown modes/sorts and malformed rows never become a different query.
    pub fn restore(value: &serde_json::Value) -> Self {
        let mut history = Self::default();
        if let Some(rows) = value.as_array() {
            for row in rows.iter().take(MAX_INSPECTED) {
                if !row
                    .get("query")
                    .and_then(|query| query.as_str())
                    .is_some_and(|query| !query.is_empty() && query.len() <= MAX_QUERY_BYTES)
                {
                    continue;
                }
                let Ok(entry) = <RecentSearch as serde::Deserialize>::deserialize(row) else {
                    continue;
                };
                if entry.valid() && !history.0.iter().any(|old| old.same_query(&entry)) {
                    history.0.push(entry);
                    if history.0.len() == MAX_ENTRIES {
                        break;
                    }
                }
            }
        }
        history
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn people_history_round_trips_without_changing_legacy_query_kinds_or_sorts() {
        let rows = json!([
            {"query":"person@example.test","kind":"people","sort":"oldest"},
            {"query":"release","kind":"messages","sort":"oldest"},
            {"query":"notes","kind":"files","sort":"newest"}
        ]);
        let history = SearchHistory::restore(&rows);
        assert_eq!(history.entries().len(), 3);
        assert_eq!(history.entries()[0].kind, SearchKind::People);
        assert_eq!(history.entries()[1].kind, SearchKind::Messages);
        assert_eq!(history.entries()[2].kind, SearchKind::Files);
        assert_eq!(history.entries()[1].sort, SearchSort::Oldest);
        assert_eq!(history.entries()[2].sort, SearchSort::Newest);
        assert_eq!(
            SearchHistory::restore(&serde_json::to_value(&history).unwrap()),
            history
        );
        assert_eq!(
            serde_json::to_value(SearchKind::People).unwrap(),
            json!("people")
        );
        assert_eq!(SearchKind::default(), SearchKind::Messages);
    }
    #[test]
    fn admitted_queries_keep_exact_syntax_and_repeat_with_new_sort_moves_to_front() {
        let mut history = SearchHistory::default();
        assert!(history.record(
            "  release  in:ship  ",
            SearchKind::Messages,
            SearchSort::Newest
        ));
        assert!(history.record("release  in:ship", SearchKind::Files, SearchSort::Newest));
        assert!(history.record("release  in:ship", SearchKind::Messages, SearchSort::Oldest));
        assert_eq!(history.entries().len(), 2);
        assert_eq!(history.entries()[0].query, "release  in:ship");
        assert_eq!(history.entries()[0].kind, SearchKind::Messages);
        assert_eq!(history.entries()[0].sort, SearchSort::Oldest);
        assert!(!history.record("release  in:ship", SearchKind::Messages, SearchSort::Oldest));
        for i in 0..25 {
            history.record(
                &format!("query {i}"),
                SearchKind::Files,
                SearchSort::Relevant,
            );
        }
        assert_eq!(history.entries().len(), 20);
        assert_eq!(history.entries()[0].query, "query 24");
        assert_eq!(history.entries()[19].query, "query 5");
    }
    #[test]
    fn query_byte_limit_is_utf8_safe_and_does_not_truncate_or_record_empty_terms() {
        let mut history = SearchHistory::default();
        let limit = "\u{1f680}".repeat(1024);
        assert_eq!(limit.len(), 4096);
        assert!(history.record(&limit, SearchKind::Files, SearchSort::Oldest));
        assert!(!history.record(
            &(limit.clone() + "x"),
            SearchKind::Files,
            SearchSort::Oldest
        ));
        assert!(!history.record(" \n\t ", SearchKind::Messages, SearchSort::Newest));
        assert_eq!(history.entries().len(), 1);
        assert_eq!(history.entries()[0].query, limit);
    }
    #[test]
    fn cache_rows_restore_newest_first_without_defaulting_unknown_modes_or_unbounded_scans() {
        let value = json!([
            {"query":"release","kind":"files","sort":"oldest"},
            {"query":"release","kind":"files","sort":"newest"},
            {"query":"other","kind":"messages","sort":"relevant"},
            {"query":"unknown mode","kind":"channels","sort":"newest"},
            {"query":"unknown sort","kind":"files","sort":"future"},
            {"query":false,"kind":"files","sort":"newest"},
            {"query":"missing sort","kind":"files"},
            {"query":" noncanonical ","kind":"files","sort":"newest"},
            null
        ]);
        let history = SearchHistory::restore(&value);
        assert_eq!(history.entries().len(), 2);
        assert_eq!(history.entries()[0].sort, SearchSort::Oldest);
        assert_eq!(history.entries()[1].query, "other");
        assert_eq!(
            SearchHistory::restore(&serde_json::to_value(&history).unwrap()),
            history
        );
        assert!(
            SearchHistory::restore(&json!({"unexpected":[]}))
                .entries()
                .is_empty()
        );
        let mut rows = vec![serde_json::Value::Null; 80];
        rows.push(value[0].clone());
        assert!(SearchHistory::restore(&json!(rows)).entries().is_empty());
    }
}
