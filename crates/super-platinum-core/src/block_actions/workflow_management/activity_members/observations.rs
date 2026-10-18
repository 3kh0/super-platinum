//! Renderer-neutral coverage and receipt validation. Parent retains the actual
//! management owner/query/locale/evaluator lease before and after every await.
use super::*;
use crate::{
    block_actions::home::workflow::{directory_local as local, directory_ranking as ranking},
    composer_emoji::{history::History, model::Model},
};
use std::{cmp::Ordering, collections::BTreeMap};
#[cfg(test)]
mod tests;

/// One immutable account/query snapshot. No cache, acquisition or default data
/// is created here; caller supplies the exact current selector/model/history set.
pub struct RankInputs<'a> {
    pub state: &'a Value,
    pub history: &'a History,
    pub model: &'a Model,
    pub now_ms: f64,
}

/// Ordered names match the getter arrays exactly. `lowered` is the renderer's
/// trim().toLocaleLowerCase(), before core J3F/whitespace normalization.
#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RendererReply {
    pub generation: String,
    pub locale: String,
    pub collation_locale: String,
    pub lowered: Vec<String>,
    pub classes: Vec<(usize, usize)>,
    pub demote_passthrough: bool,
}

pub struct ObservationPlan {
    generation: String,
    query: String,
    rows: Vec<Value>,
    fields: Vec<String>,
    names: Vec<String>,
}
impl ObservationPlan {
    pub fn capture(
        generation: &str,
        query: &Query,
        path: SearchPath,
        matched: &[local::Scored<'_>],
        comparison_names: &[String],
    ) -> Result<Self, ranking::Error> {
        if generation.is_empty() {
            return Err(ranking::Error::InvalidInput);
        }
        let rows = super::ordering::rows(matched, comparison_names)?;
        let query = query.text(path);
        let stripped = query
            .strip_prefix([
                ':', '@', '\u{ff20}', '#', '\u{ff03}', '/', '\u{ff0f}', '+', '\u{ff0b}',
            ])
            .unwrap_or(query);
        let mut fields = BTreeSet::from([String::new(), query.to_owned(), stripped.to_owned()]);
        let mut names = BTreeSet::new();
        for row in &rows {
            let item = &row["item"];
            let facts = &item["facts"];
            // This leaf accepts member rows, never forged mixed entity types.
            if !facts["channelType"].is_null() || facts["mpim"] == true {
                return Err(ranking::Error::InvalidInput);
            }
            let text = |v: &Value| v.as_str().unwrap_or("").to_owned();
            let raw = text(&item["name"]);
            let comparison = text(&facts["comparisonName"]);
            names.insert(if comparison.is_empty() {
                raw.clone()
            } else {
                comparison
            });
            let best = text(&facts["bestName"]);
            let best = if best.is_empty() { raw.clone() } else { best };
            // l24D member name, jhqG normalized first/last-name features,
            // newline NAV_CONTAINS_QUERY and optional passthrough demotion.
            for field in [
                raw,
                best,
                text(&facts["firstName"]),
                text(&facts["lastName"]),
            ] {
                fields.extend(field.split('\n').map(str::to_owned));
                fields.insert(field);
            }
        }
        Ok(Self {
            generation: generation.into(),
            query: query.into(),
            rows,
            fields: fields.into_iter().collect(),
            names: names.into_iter().collect(),
        })
    }
    pub fn generation(&self) -> &str {
        &self.generation
    }
    pub fn normalization_fields(&self) -> &[String] {
        &self.fields
    }
    pub fn collation_names(&self) -> &[String] {
        &self.names
    }
    pub fn decode(
        self,
        expected_locale: &str,
        reply: RendererReply,
    ) -> Result<RankObservations, ranking::Error> {
        if reply.generation != self.generation
            || expected_locale.is_empty()
            || reply.locale != expected_locale
            || reply.collation_locale != "en-US"
            || reply.lowered.len() != self.fields.len()
            || reply.classes.len() != self.names.len()
        {
            return Err(ranking::Error::InvalidInput);
        }
        let mut seen = BTreeSet::new();
        let mut classes = BTreeMap::new();
        let mut previous = 0;
        for (position, (index, class)) in reply.classes.into_iter().enumerate() {
            if index >= self.names.len()
                || !seen.insert(index)
                || (position == 0 && class != 0)
                || class < previous
                || class > previous + 1
            {
                return Err(ranking::Error::InvalidInput);
            }
            previous = class;
            classes.insert(self.names[index].clone(), class);
        }
        let normalized = self
            .fields
            .into_iter()
            .zip(reply.lowered)
            .map(|(source, lowered)| (source, local::normalize_lowered_query(&lowered)))
            .collect();
        Ok(RankObservations {
            generation: self.generation,
            locale: reply.locale,
            query: self.query,
            rows: self.rows,
            normalized,
            classes,
            demote: reply.demote_passthrough,
        })
    }
}

pub struct RankObservations {
    generation: String,
    locale: String,
    query: String,
    rows: Vec<Value>,
    normalized: BTreeMap<String, String>,
    classes: BTreeMap<String, usize>,
    demote: bool,
}
impl RankObservations {
    pub fn generation(&self) -> &str {
        &self.generation
    }
    pub fn locale(&self) -> &str {
        &self.locale
    }
    pub fn demote_passthrough(&self) -> bool {
        self.demote
    }
    pub(super) fn validate(&self, query: &str, rows: &[Value]) -> Result<(), ranking::Error> {
        if query != self.query || rows != self.rows {
            return Err(ranking::Error::InvalidInput);
        }
        Ok(())
    }
    pub(super) fn normalize(&self, text: &str) -> Result<String, ranking::Error> {
        self.normalized
            .get(text)
            .cloned()
            .ok_or_else(|| ranking::Error::MissingObservation(text.into()))
    }
    pub(super) fn compare(&self, a: &str, b: &str) -> Option<Ordering> {
        Some(self.classes.get(a)?.cmp(self.classes.get(b)?))
    }
}
