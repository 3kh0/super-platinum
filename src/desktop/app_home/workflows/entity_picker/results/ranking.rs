//! Preserve selection custody while the pure source ranker orders original rows.
//! Group flattening has a different index order from acquisition: keep an exact
//! index map and never reconstruct a Choice or proof from ranked JSON.
use super::*;
use super_platinum_core::{
    block_actions::home::workflow::directory_ranking as ranked, composer_emoji::history::History,
};

/// Built from the caller-owned selector snapshot. `items` includes application
/// facts, never the remote `facts` object; IDs must match retained Choices.
pub(super) struct Input {
    pub(super) cache: ranking_cache::Snapshot,
    pub(super) items: Vec<Value>,
    pub(super) selectors: Value,
    pub(super) history: History,
    pub(super) now_ms: f64,
}
struct Groups {
    rows: Value,
    original: Vec<usize>,
    ids: Vec<String>,
}
fn groups(choices: &[Choice], items: Vec<Value>) -> Result<Groups, Error> {
    if choices.len() != items.len() {
        return Err(unavailable());
    }
    let mut rows = serde_json::json!({"members":[],"channels":[],"mpims":[],"ims":[]});
    let mut original = Vec::new();
    let mut ids = Vec::new();
    for (kind, group) in [
        ("member", "members"),
        ("channel", "channels"),
        ("mpim", "mpims"),
        ("ims", "ims"),
    ] {
        for (index, (choice, item)) in choices.iter().zip(&items).enumerate() {
            if choice.group != kind {
                continue;
            }
            let id = match &choice.entity {
                Entity::User { user, .. } => user.user().id.as_str(),
                Entity::Conversation { raw, .. } => raw["id"].as_str().ok_or_else(unavailable)?,
            };
            if item["id"].as_str() != Some(id) || !item["facts"].is_object() {
                return Err(unavailable());
            }
            let mut row = serde_json::json!({"id":id,"item":item});
            if let Some(m) = &choice.matching {
                // Explicit null is Infinity. Omitted score is a distinct source
                // state, retained only for an actually unmatched remote row.
                row["score"] = serde_json::json!(m.score);
                if let Some(details) = &m.score_details {
                    row["scoreDetails"] = serde_json::json!(details);
                }
                if let Some(exact) = m.is_exact_match {
                    row["isExactMatch"] = exact.into();
                }
                if let Some(details) = &m.exact_match_details {
                    row["exactMatchDetails"] = serde_json::json!(details);
                }
                row["isSelf"] = m.is_self.into();
                row["isFlannelFuzzyPassthrough"] = m.is_flannel_fuzzy_passthrough.into();
            }
            if let Some(previous) = &choice.previous {
                if let Some(scores) = &previous.scores {
                    row["previousNameScores"] = Value::Array(
                        scores
                            .iter()
                            .map(|(name, score)| serde_json::json!({"name":name,"score":score}))
                            .collect(),
                    );
                }
                if let Some(best) = &previous.best {
                    row["bestPreviousNameMatch"] = best.clone().into();
                }
            }
            if let Some(m) = &choice.mpim {
                if let Some(by_member) = &m.score_details_by_member {
                    row["scoreDetailsByMember"] = Value::Object(
                        by_member
                            .iter()
                            .map(|(id, details)| {
                                (id.clone(), serde_json::json!({"score_details":details}))
                            })
                            .collect(),
                    );
                }
                if let Some(subqueries) = &m.subquery_scores_by_member {
                    row["subQueryScoresByMember"] = Value::Array(
                        subqueries
                            .iter()
                            .map(|members| {
                                Value::Array(
                                    members
                                        .iter()
                                        .map(|m| serde_json::json!({"id":m.id,"score":m.score}))
                                        .collect(),
                                )
                            })
                            .collect(),
                    );
                }
            }
            rows[group].as_array_mut().unwrap().push(row);
            original.push(index);
            ids.push(id.to_owned());
        }
    }
    if original.len() != choices.len() {
        return Err(unavailable());
    }
    Ok(Groups {
        rows,
        original,
        ids,
    })
}

fn truth(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|n| n != 0.0),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}
fn text(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}
fn either<'a>(a: &'a Value, b: &'a Value) -> &'a str {
    if truth(a) { text(a) } else { text(b) }
}
/// Bounded complete superset from exact 4ucY initial and l24D prepared names.
/// Names are strings, not IDs: repeated identities do not erase distinct names.
fn observed_strings(query: &str, items: &[Value]) -> (BTreeSet<String>, BTreeSet<String>) {
    let stripped = query
        .strip_prefix([
            ':', '@', '\u{ff20}', '#', '\u{ff03}', '/', '\u{ff0f}', '+', '\u{ff0b}',
        ])
        .unwrap_or(query);
    let mut fields = BTreeSet::from([String::new(), query.to_owned(), stripped.to_owned()]);
    let mut names = BTreeSet::new();
    for item in items {
        let f = &item["facts"];
        let comparison = if truth(&f["mpim"]) {
            text(&item["name_normalized"]).to_owned()
        } else if truth(&f["channelType"]) {
            either(&f["comparisonName"], &item["name"]).to_lowercase()
        } else if truth(&f["member"]) {
            either(&f["comparisonName"], &item["name"]).to_owned()
        } else {
            text(&item["name"]).to_owned()
        };
        names.insert(comparison);
        // Include both possible prepared branches even when a source group
        // overrides its object's selector kind. Empty/newline segments matter.
        for name in [
            text(&item["name"]),
            either(&item["name"], &item["string"]),
            either(&f["bestName"], &item["name"]),
            if truth(&f["customNameAllowed"]) && truth(&f["customName"]) {
                text(&f["customName"])
            } else {
                either(&f["mpimName"], &item["name"])
            },
        ] {
            fields.insert(name.to_owned());
            fields.extend(name.split('\n').map(str::to_owned));
        }
    }
    (fields, names)
}

/// Exact guarded renderer observations are acquired before the CPU phase. Only
/// immutable JSON/model/history enter spawn_blocking; no Signal or native lease
/// is used by that thread. Selection evidence never leaves the caller.
pub(super) async fn order(
    state: Signal<ShellState>,
    query: &entity_search::Query,
    choices: Vec<Choice>,
    input: Input,
) -> Result<Vec<Choice>, Error> {
    if query.is_validation() {
        return Err(unavailable());
    }
    if !query.current(&state.peek()) {
        return Err(unavailable());
    }
    let cache = input.cache.clone();
    if !cache.current(&state.peek()) {
        return Err(unavailable());
    }
    let (fields, names) = observed_strings(&query.text, &input.items);
    let casing = entity_normalization::capture(state, query, BTreeSet::new()).await?;
    let normalizations = entity_normalization::fields(state, query, &casing, fields).await?;
    let collation = ranking_renderer::capture(state, query, names, casing.locale()).await?;
    if !query.current(&state.peek()) || !cache.current(&state.peek()) {
        return Err(unavailable());
    }
    let grouped = groups(&choices, input.items)?;
    let raw_query = query.text.clone();
    let options = ranked::Options {
        limit: if query.members() { 24 } else { 100 },
        source: if query.members() {
            String::new()
        } else if query.custom {
            "workflow-speedbump-missing-customizable-input-modal".into()
        } else {
            "workflow-speedbump-missing-context-modal".into()
        },
        demote_passthrough: collation.demote_passthrough,
        ..Default::default()
    };
    let worker_cache = cache.clone();
    let indexes = tokio::task::spawn_blocking(move || {
        // The locale witness remains captured for this query; no normalization
        // is performed by Rust's default lowercase or a fallback comparator.
        let _locale = casing.locale();
        let normalize = |s: &str| {
            normalizations
                .get(s)
                .cloned()
                .ok_or_else(|| ranked::Error::MissingObservation(s.into()))
        };
        let context = ranked::Context {
            state: &input.selectors,
            history: &input.history,
            model: &worker_cache.model,
            now_ms: input.now_ms,
            normalize: &normalize,
        };
        let missing = std::cell::Cell::new(false);
        let mut similarity = worker_cache.similarity.lock().map_err(|_| unavailable())?;
        let result = ranked::rank(
            &raw_query,
            &grouped.rows,
            &options,
            &context,
            &mut similarity,
            |a, b| {
                collation.compare(a, b).unwrap_or_else(|| {
                    missing.set(true);
                    std::cmp::Ordering::Equal
                })
            },
        )
        .map_err(|_| unavailable())?;
        if missing.get() {
            return Err(unavailable());
        }
        let mut used = BTreeSet::new();
        result
            .into_iter()
            .map(|row| {
                let i = row.original_index;
                if !used.insert(i)
                    || grouped.ids.get(i).map(String::as_str) != row.row["id"].as_str()
                {
                    return Err(unavailable());
                }
                grouped.original.get(i).copied().ok_or_else(unavailable)
            })
            .collect::<Result<Vec<_>, Error>>()
    })
    .await
    .map_err(|_| unavailable())??;
    if !query.current(&state.peek()) || !cache.current(&state.peek()) {
        return Err(unavailable());
    }
    retain_originals(choices, indexes)
}
fn retain_originals<T>(values: Vec<T>, indexes: Vec<usize>) -> Result<Vec<T>, Error> {
    let mut slots: Vec<_> = values.into_iter().map(Some).collect();
    indexes
        .into_iter()
        .map(|i| {
            slots
                .get_mut(i)
                .and_then(Option::take)
                .ok_or_else(unavailable)
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_prepared_name_and_newline_segment_has_an_exact_observation() {
        let items = vec![
            serde_json::json!({"name":"Raw\n", "string":"Other", "name_normalized":"Group",
            "facts":{"member":true,"comparisonName":"Shown","bestName":"Best\nSecond"}}),
            serde_json::json!({"name":"Base", "name_normalized":"Group", "facts":{"mpim":true,"customNameAllowed":true,"customName":"Custom"}}),
        ];
        let (fields, names) = observed_strings(":#ada", &items);
        for s in [
            ":#ada",
            "#ada",
            "Raw\n",
            "Raw",
            "",
            "Best\nSecond",
            "Best",
            "Second",
            "Custom",
        ] {
            assert!(fields.contains(s), "{s:?}");
        }
        assert_eq!(names, BTreeSet::from(["Shown".into(), "Group".into()]));
        assert!(!fields.contains("ada"));
    }
    #[test]
    fn original_custody_is_moved_once_and_bad_indexes_refuse() {
        let original: Vec<_> = (0..150).map(|_| Arc::new(())).collect();
        let expected = original[140].clone();
        let ordered = retain_originals(original, vec![140, 2, 0]).unwrap();
        assert!(Arc::ptr_eq(&ordered[0], &expected));
        assert!(retain_originals(vec![1, 2], vec![1, 1]).is_err());
        assert!(retain_originals(vec![1, 2], vec![2]).is_err());
    }
}
