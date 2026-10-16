//! Edits explicit search modifiers without rewriting the user's free-form query.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchFilters {
    pub channel: String,
    pub sender: String,
    pub after: String,
    pub before: String,
    pub has_file: bool,
    pub has_link: bool,
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
            has_file: has_token(query, "has:file"),
            has_link: has_token(query, "has:link"),
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
        for (token, enabled, old) in [
            ("has:file", self.has_file, original.has_file),
            ("has:link", self.has_link, original.has_link),
        ] {
            if enabled != old {
                query = toggle_token(&query, token, enabled)?;
            }
        }
        Ok(query)
    }
}

// Only canonical positive tokens are owned by these controls. Quoted spellings,
// exclusions and other has values remain raw syntax, never guessed/normalized.
fn has_token(query: &str, token: &str) -> bool {
    tokens(query)
        .into_iter()
        .any(|(start, end)| &query[start..end] == token)
}

fn toggle_token(query: &str, token: &str, enabled: bool) -> Result<String, &'static str> {
    let mut result = query.to_owned();
    if enabled {
        let mut quoted = false;
        let mut escaped = false;
        for ch in query.chars() {
            if ch == '"' && !escaped {
                quoted = !quoted;
            }
            escaped = ch == '\\' && !escaped;
        }
        // Appending inside an unfinished phrase would add literal text instead
        // of the requested modifier. Keep the original query on refusal.
        if quoted {
            return Err("Close the quoted phrase before adding a filter.");
        }
        if query.chars().last().is_some_and(|ch| !ch.is_whitespace()) {
            result.push(' ');
        }
        result.push_str(token);
    } else {
        for (start, end) in tokens(query).into_iter().rev() {
            if &query[start..end] == token {
                result.replace_range(start..end, "");
            }
        }
    }
    Ok(result)
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
    #[test]
    fn has_flags_project_only_exact_positive_tokens() {
        let query = r#"has:file -has:link has:pin has::eyes: HAS:link has:links has:filetype"#;
        let filters = SearchFilters::from_query(query);
        assert!(filters.has_file);
        assert!(!filters.has_link);
        assert_eq!(filters.apply(query).unwrap(), query);
    }
    #[test]
    fn has_unchanged_repeated_quoted_excluded_and_unknown_round_trip_exactly() {
        for query in [
            "  has:file\thas:link has:file  ",
            r#""has:file" "say \"has:link\"" -has:file -has:link"#,
            r#"has:"file" has:"link" has:pin has::eyes: is:thread"#,
            "\u{1f680}\u{2003}has:file\nfrom:me before:tomorrow",
            r#"unfinished "has:file has:link"#,
        ] {
            assert_eq!(
                SearchFilters::from_query(query).apply(query).unwrap(),
                query
            );
        }
    }
    #[test]
    fn removing_file_preserves_link_exclusions_literals_and_other_has_values() {
        let query = r#"x has:file has:link has:file -has:file "has:file" has:"file" has:pin"#;
        let mut filters = SearchFilters::from_query(query);
        filters.has_file = false;
        assert_eq!(
            filters.apply(query).unwrap(),
            r#"x  has:link  -has:file "has:file" has:"file" has:pin"#
        );
    }
    #[test]
    fn removing_link_preserves_file_and_whitespace() {
        let query = "\thas:link  has:file has:link\n-has:link ";
        let mut filters = SearchFilters::from_query(query);
        filters.has_link = false;
        assert_eq!(filters.apply(query).unwrap(), "\t  has:file \n-has:link ");
    }
    #[test]
    fn adding_both_retains_raw_bytes_and_uses_existing_separator() {
        let query = "\u{1f680} \"exact phrase\" has:pin -has:file\t";
        let filters = SearchFilters {
            has_file: true,
            has_link: true,
            ..SearchFilters::from_query(query)
        };
        let applied = filters.apply(query).unwrap();
        assert_eq!(applied, format!("{query}has:file has:link"));
        assert_eq!(
            SearchFilters::from_query(&applied).apply(&applied).unwrap(),
            applied
        );
    }
    #[test]
    fn adding_one_flag_never_replaces_repeated_other_flag() {
        let query = "has:file has:file";
        let mut filters = SearchFilters::from_query(query);
        filters.has_link = true;
        assert_eq!(filters.apply(query).unwrap(), "has:file has:file has:link");
    }
    #[test]
    fn clear_owned_flags_retains_unmanaged_query_syntax() {
        let query = r#"report has:file has:link -has:file has:pin has:"link" "has:file""#;
        assert_eq!(
            SearchFilters::default().apply(query).unwrap(),
            r#"report   -has:file has:pin has:"link" "has:file""#
        );
    }
    #[test]
    fn adding_flag_to_unfinished_quote_refuses_without_rewriting_query() {
        let query = r#"report "unfinished"#;
        let mut filters = SearchFilters::from_query(query);
        filters.has_file = true;
        assert_eq!(
            filters.apply(query),
            Err("Close the quoted phrase before adding a filter.")
        );
        assert_eq!(query, r#"report "unfinished"#);
    }
    #[test]
    fn has_changes_compose_with_existing_fields_and_date_validation() {
        let query = "has:link in:old after:yesterday";
        let mut filters = SearchFilters::from_query(query);
        filters.channel = "ship".into();
        filters.has_file = true;
        filters.has_link = false;
        assert_eq!(
            filters.apply(query).unwrap(),
            "  after:yesterday in:ship has:file"
        );
        filters.after = "2026-02-30".into();
        assert!(filters.apply(query).is_err());
    }
    #[test]
    fn empty_query_add_and_remove_have_no_hidden_normalization() {
        let filters = SearchFilters {
            has_file: true,
            ..Default::default()
        };
        assert_eq!(filters.apply("").unwrap(), "has:file");
        assert_eq!(SearchFilters::default().apply("has:file").unwrap(), "");
        let filters = SearchFilters {
            has_link: true,
            ..Default::default()
        };
        assert_eq!(filters.apply("").unwrap(), "has:link");
    }
}
