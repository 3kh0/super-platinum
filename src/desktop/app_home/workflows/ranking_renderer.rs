//! Owned renderer collation, captured once for all comparison names. No DOM or
//! Slack state is read by the JavaScript helper; Rust retains query ownership.
use super::*;
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
};

pub(super) struct Collation {
    classes: BTreeMap<String, usize>,
    pub(super) demote_passthrough: bool,
}
impl Collation {
    // Missing observations are not ASCII/equal fallbacks. The rank adapter must
    // establish coverage before entering its infallible comparator callback.
    pub(super) fn compare(&self, a: &str, b: &str) -> Option<Ordering> {
        Some(self.classes.get(a)?.cmp(self.classes.get(b)?))
    }
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    generation: String,
    locale: String,
    #[serde(rename = "collationLocale")]
    collation_locale: String,
    #[serde(rename = "demotePassthrough")]
    demote_passthrough: bool,
    classes: Vec<(usize, usize)>,
}
pub(super) async fn capture(
    state: Signal<ShellState>,
    query: &entity_search::Query,
    names: BTreeSet<String>,
    locale: &str,
) -> Result<Collation, Error> {
    if !query.current(&state.peek()) {
        return Err(unavailable());
    }
    let names: Vec<_> = names.into_iter().collect();
    let generation = query.generation.to_string();
    let request = serde_json::json!({"generation":generation,"names":names});
    let script = format!(
        "{}\ndioxus.send(workflowRankingCollation({}));await dioxus.recv();",
        include_str!("ranking_renderer.js"),
        request
    );
    let mut eval = dioxus::document::eval(&script);
    let result = tokio::time::timeout(Duration::from_secs(5), eval.recv::<Option<Reply>>()).await;
    let _ = eval.send(());
    if !query.current(&state.peek()) {
        return Err(unavailable());
    }
    let reply = result
        .map_err(|_| unavailable())?
        .map_err(|_| unavailable())?
        .ok_or_else(unavailable)?;
    decode(&generation, locale, &names, reply)
}
fn decode(
    generation: &str,
    locale: &str,
    names: &[String],
    reply: Reply,
) -> Result<Collation, Error> {
    if reply.generation != generation
        || reply.locale != locale
        || locale.is_empty()
        || reply.collation_locale != "en-US"
        || reply.classes.len() != names.len()
    {
        return Err(unavailable());
    }
    let mut seen = BTreeSet::new();
    let mut classes = BTreeMap::new();
    let mut previous = 0;
    for (position, (index, class)) in reply.classes.into_iter().enumerate() {
        if index >= names.len()
            || !seen.insert(index)
            || (position == 0 && class != 0)
            || class < previous
            || class > previous + 1
        {
            return Err(unavailable());
        }
        previous = class;
        classes.insert(names[index].clone(), class);
    }
    Ok(Collation {
        classes,
        demote_passthrough: reply.demote_passthrough,
    })
}
fn unavailable() -> Error {
    Error::Api("home_workflow_collation_unavailable".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn reply(classes: Vec<(usize, usize)>) -> Reply {
        Reply {
            generation: "7".into(),
            locale: "en-US".into(),
            collation_locale: "en-US".into(),
            demote_passthrough: false,
            classes,
        }
    }
    #[test]
    fn exact_ties_and_numeric_order_remain_observed() {
        let names = ["channel10", "channel2", "channel02"].map(str::to_owned);
        let c = decode("7", "en-US", &names, reply(vec![(1, 0), (2, 0), (0, 1)])).unwrap();
        assert_eq!(c.compare("channel2", "channel02"), Some(Ordering::Equal));
        assert_eq!(c.compare("channel10", "channel2"), Some(Ordering::Greater));
        assert_eq!(c.compare("missing", "channel2"), None);
    }
    #[test]
    fn stale_locale_generation_and_incomplete_permutations_refuse() {
        let names = ["a".into(), "b".into()];
        for classes in [
            vec![(0, 0)],
            vec![(0, 0), (0, 1)],
            vec![(0, 0), (2, 1)],
            vec![(0, 1), (1, 1)],
            vec![(0, 0), (1, 2)],
        ] {
            assert!(decode("7", "en-US", &names, reply(classes)).is_err());
        }
        assert!(decode("8", "en-US", &names, reply(vec![(0, 0), (1, 1)])).is_err());
        assert!(decode("7", "tr", &names, reply(vec![(0, 0), (1, 1)])).is_err());
    }
}
