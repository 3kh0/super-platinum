//! Edits explicit search modifiers without rewriting the user's free-form query.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchFilters {
    pub channel: String,
    pub sender: String,
    pub after: String,
    pub before: String,
}

impl SearchFilters {
    pub fn from_query(query: &str) -> Self {
        let get = |key: &str| {
            tokens(query)
                .into_iter()
                .find_map(|(start, end)| {
                    query[start..end]
                        .strip_prefix(&format!("{key}:"))
                        .map(|s| s.trim_matches('"').to_owned())
                })
                .unwrap_or_default()
        };
        Self {
            channel: get("in"),
            sender: get("from"),
            after: get("after"),
            before: get("before"),
        }
    }

    pub fn apply(&self, query: &str) -> Result<String, &'static str> {
        let original = Self::from_query(query);
        for (value, old) in [
            (&self.after, &original.after),
            (&self.before, &original.before),
        ] {
            if value != old
                && !value.trim().is_empty()
                && chrono::NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d").is_err()
            {
                return Err("Use YYYY-MM-DD for dates.");
            }
        }
        let mut query = query.to_owned();
        for (key, value, old) in [
            ("in", &self.channel, &original.channel),
            ("from", &self.sender, &original.sender),
            ("after", &self.after, &original.after),
            ("before", &self.before, &original.before),
        ] {
            if value != old {
                query = replace(&query, key, value)?;
            }
        }
        Ok(query)
    }
}

fn replace(query: &str, key: &str, value: &str) -> Result<String, &'static str> {
    let value = value.trim();
    // Field values cannot inject another modifier or unbalance a quoted phrase.
    if value.contains(['"', '\\', '\n', '\r']) {
        return Err("Use a channel or username without quotes or line breaks.");
    }
    let prefix = format!("{key}:");
    let mut result = query.to_owned();
    for (start, end) in tokens(query).into_iter().rev() {
        if query[start..end].starts_with(&prefix) {
            result.replace_range(start..end, "");
        }
    }
    let result = result.trim_end();
    if value.is_empty() {
        return Ok(result.to_owned());
    }
    let value = if value.chars().any(char::is_whitespace) {
        format!("\"{value}\"")
    } else {
        value.into()
    };
    Ok(format!(
        "{result}{}{key}:{value}",
        if result.is_empty() { "" } else { " " }
    ))
}

// Preserve byte boundaries, whitespace, negative modifiers, and quoted literals.
fn tokens(query: &str) -> Vec<(usize, usize)> {
    let mut result = Vec::new();
    let mut start = None;
    let mut quoted = false;
    let mut escaped = false;
    for (index, ch) in query.char_indices() {
        if ch.is_whitespace() && !quoted {
            if let Some(start) = start.take() {
                result.push((start, index));
            }
        } else {
            start.get_or_insert(index);
        }
        if ch == '"' && !escaped {
            quoted = !quoted;
        }
        escaped = ch == '\\' && !escaped;
    }
    if let Some(start) = start {
        result.push((start, query.len()));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn editing_channel_preserves_quotes_exclusions_and_other_operators() {
        let query = "🚀 \"in:literal phrase\" -in:noise in:old has:file from:<@U1>";
        let mut filters = SearchFilters::from_query(query);
        assert_eq!(filters.channel, "old");
        assert_eq!(filters.sender, "<@U1>");
        filters.channel = "ship".into();
        assert_eq!(
            filters.apply(query).unwrap(),
            "🚀 \"in:literal phrase\" -in:noise  has:file from:<@U1> in:ship"
        );
    }
    #[test]
    fn unchanged_fields_round_trip_multiple_values_and_relative_dates() {
        let query = "in:a in:b after:yesterday before:tomorrow \"exact phrase\"";
        assert_eq!(
            SearchFilters::from_query(query).apply(query).unwrap(),
            query
        );
    }
    #[test]
    fn filter_values_cannot_inject_query_operators() {
        let filters = SearchFilters {
            channel: "ship from:other".into(),
            ..Default::default()
        };
        assert_eq!(
            filters.apply("launch").unwrap(),
            "launch in:\"ship from:other\""
        );
        assert!(
            SearchFilters {
                sender: "\" from:other".into(),
                ..Default::default()
            }
            .apply("launch")
            .is_err()
        );
    }
    #[test]
    fn dates_validate_and_clearing_preserves_other_syntax() {
        let mut filters = SearchFilters::from_query("report after:2026-01-01 has:link");
        filters.after = "2026-02-30".into();
        assert!(filters.apply("report after:2026-01-01 has:link").is_err());
        filters.after.clear();
        filters.before = "2026-10-12".into();
        assert_eq!(
            filters.apply("report after:2026-01-01 has:link").unwrap(),
            "report  has:link before:2026-10-12"
        );
    }
    #[test]
    fn quoted_values_round_trip_without_touching_escaped_phrases() {
        let query = "\"say \\\"from:test\\\"\" from:\"Jane Doe\"";
        assert_eq!(SearchFilters::from_query(query).sender, "Jane Doe");
        assert_eq!(
            SearchFilters::from_query(query).apply(query).unwrap(),
            query
        );
    }
}
