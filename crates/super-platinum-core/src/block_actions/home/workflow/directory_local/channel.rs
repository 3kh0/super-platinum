//! Exact hHTo channel scoring; display-name acquisition remains source-owned.
use super::*;

#[derive(Clone, Debug, PartialEq)]
pub struct PreviousNameScore<'a> {
    pub name: &'a str,
    /// None represents JavaScript Infinity, not zero or an omitted score.
    pub score: Option<u32>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ChannelScored<'a> {
    pub item: &'a Value,
    pub id: &'a str,
    /// A previous-name match may admit an infinite current-name score.
    pub score: Option<u32>,
    pub is_exact_match: Option<bool>,
    pub previous_name_scores: Option<Vec<PreviousNameScore<'a>>>,
    pub best_previous_name_match: Option<&'a str>,
    pub is_flannel_fuzzy_passthrough: bool,
}
fn plain_channel<'a>(item: &'a Value, id: &'a str) -> ChannelScored<'a> {
    ChannelScored {
        item,
        id,
        score: Some(0),
        is_exact_match: None,
        previous_name_scores: None,
        best_previous_name_match: None,
        is_flannel_fuzzy_passthrough: false,
    }
}
/// dfPN.SB inputs for ordinary channel-store candidates. File/huddle/unknown
/// records are rejected before name acquisition; IM/MPIM use their own helpers.
/// These are acquisition requirements, never guessed display-name fallbacks.
#[derive(Clone, Debug, PartialEq)]
pub enum ChannelNameInput<'a> {
    Ordinary {
        raw_name: Option<&'a str>,
    },
    Record {
        raw_name_or_ellipsis: &'a str,
    },
    Meeting {
        raw_name: Option<&'a str>,
        event_link_id: Option<&'a str>,
    },
}
pub fn channel_name_input(item: &Value) -> Option<ChannelNameInput<'_>> {
    if excluded(item) || truthy(&item["is_im"]) || truthy(&item["is_mpim"]) {
        return None;
    }
    let raw_name = item["name"].as_str();
    if truthy(&item["properties"]["record_channel"]["record_id"]) {
        return Some(ChannelNameInput::Record {
            raw_name_or_ellipsis: raw_name.filter(|s| !s.is_empty()).unwrap_or("..."),
        });
    }
    if truthy(&item["properties"]["meetings_channel"]["is_meeting_channel"])
        || raw_name.is_some_and(|s| s.starts_with("mc:"))
    {
        return Some(ChannelNameInput::Meeting {
            raw_name,
            event_link_id: item["properties"]["meetings_channel"]["event_link_id"].as_str(),
        });
    }
    Some(ChannelNameInput::Ordinary { raw_name })
}
fn excluded(item: &Value) -> bool {
    !known(item)
        || truthy(&item["properties"]["huddles"]["is_huddle_channel"])
        || truthy(&item["is_file"])
}
/// Preserve original IDs/order and source metadata, before 4ucY/l24D sorting.
/// `display_name` supplies hNDQ(dfPN.SB(state,item,{publicChannelIncludeHashSign:false})).
/// `previous_name` supplies hNDQ(raw previous name), under the same query owner.
/// Both callbacks must be complete acquired values, not fallback normalization.
/// `eligible` is the actual global and channel-specific 8Bbq predicate.
pub fn filter_channels<'a>(
    items: &'a [Value],
    options: &Options<'_>,
    search_previous_names: bool,
    remote_channels: &BTreeSet<String>,
    eligible: impl Fn(&Value) -> bool,
    display_name: impl Fn(&Value) -> String,
    previous_name: impl Fn(&str) -> String,
) -> Vec<ChannelScored<'a>> {
    let matcher = FuzzyMatcher::new(options.matcher_query, options.fuzzy);
    items
        .iter()
        .filter_map(|item| {
            if excluded(item) || !eligible(item) {
                return None;
            }
            let id = item["id"].as_str()?;
            let mut row = plain_channel(item, id);
            if options.normalized_query.is_empty() && options.allow_empty_query {
                return Some(row);
            }
            let name = display_name(item);
            row.score = matcher.score(&name);
            if options.prefer_exact_match {
                row.is_exact_match =
                    Some(!options.normalized_query.is_empty() && name == options.normalized_query);
            }
            let current_match = row.score.is_some_and(|n| n <= options.fuzzy.limit);
            let mut prior_match = false;
            if let Some(names) = item["previous_names"]
                .as_array()
                .filter(|names| search_previous_names && !names.is_empty())
            {
                let scores: Vec<_> = names
                    .iter()
                    .filter_map(Value::as_str)
                    .filter(|name| !name.starts_with("mpdm-"))
                    .map(|name| {
                        let normalized = previous_name(name);
                        let score = if normalized == options.matcher_query {
                            Some(0)
                        } else {
                            matcher.score(&normalized)
                        };
                        PreviousNameScore { name, score }
                    })
                    .collect();
                prior_match = scores
                    .iter()
                    .any(|p| p.score.is_some_and(|n| n <= options.fuzzy.limit));
                if prior_match && !current_match {
                    row.best_previous_name_match = scores
                        .iter()
                        .filter_map(|p| p.score.map(|score| (score, p.name)))
                        .min_by_key(|p| p.0)
                        .map(|p| p.1);
                }
                row.previous_name_scores = Some(scores);
            }
            if current_match || prior_match {
                Some(row)
            } else if options.passthrough_remote_fuzzy_filtering && remote_channels.contains(id) {
                row.score = Some(2);
                row.is_flannel_fuzzy_passthrough = true;
                Some(row)
            } else {
                (options.raw_query == id).then(|| plain_channel(item, id))
            }
        })
        .collect()
}

#[cfg(test)]
mod tests;
