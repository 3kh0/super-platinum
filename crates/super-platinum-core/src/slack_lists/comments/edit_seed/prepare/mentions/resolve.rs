//! RZMQ/8KmV/1GCd selection and fhSo splice. All workspace-dependent
//! search/name/normalization/history facts are explicit original observations.
use super::*;
use crate::composer_emoji::history::History;
use std::collections::BTreeMap;
mod member;
#[cfg(test)]
mod tests;

#[derive(Clone, Debug)]
pub struct MentionSearch {
    pub lookup: MentionLookup,
    /// Actual ordered 4wTh results, including a positively observed empty list.
    pub rows: Vec<Value>,
}
#[derive(Clone, Debug)]
pub struct MentionNames {
    pub raw: Value,
    /// Exact 5cyv.KX/KP outputs, including @ and positively empty secondary.
    pub primary: String,
    pub secondary: String,
}
#[derive(Clone, Debug, Default)]
pub struct MentionObservations {
    pub searches: Vec<MentionSearch>,
    pub names: Vec<MentionNames>,
    /// Exact 8KmV.Pc (J3F then lowercase), not trimmed hNDQ normalization.
    pub normalizations: BTreeMap<String, String>,
    pub history: Option<History>,
    /// Exact tyx1 hNDQ/Ojx4 normalization for original candidate labels.
    pub history_queries: BTreeMap<String, String>,
    pub now_ms: Option<f64>,
    pub highlight_words: Option<Vec<String>>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum MentionNeed {
    Search(MentionLookup),
    Names(Value),
    Normalization(String),
    History,
    HistoryQuery(String),
    Clock,
    Highlights,
}
#[derive(Clone, Debug, PartialEq)]
pub struct MentionChoice {
    pub candidate_index: usize,
    pub raw: Value,
    pub id: String,
    pub label: String,
    pub global_index_utf16: usize,
    pub length_utf16: usize,
}
#[derive(Clone, Debug, PartialEq)]
pub enum MentionStep {
    Need(MentionNeed),
    Complete {
        ops: Vec<Value>,
        choices: Vec<MentionChoice>,
    },
}
struct Context<'a> {
    observed: &'a MentionObservations,
    need: Option<MentionNeed>,
}
impl Context<'_> {
    fn missing<T>(&mut self, need: MentionNeed) -> Result<T> {
        self.need = Some(need);
        Err(SeedUnavailable::InvalidObservation)
    }
    fn normalize(&mut self, text: &str) -> Result<String> {
        if text.len() > 262_144 {
            return Err(SeedUnavailable::Limit);
        }
        if text.is_empty() {
            return Ok(String::new());
        }
        match self.observed.normalizations.get(text) {
            Some(value) if value.len() <= 262_144 => Ok(value.clone()),
            Some(_) => Err(SeedUnavailable::Limit),
            None => self.missing(MentionNeed::Normalization(text.into())),
        }
    }
}
impl MentionPlan {
    pub fn advance(&self, observed: &MentionObservations) -> Result<MentionStep> {
        let mut context = Context {
            observed,
            need: None,
        };
        match resolve(self, &mut context) {
            Ok((ops, choices)) => Ok(MentionStep::Complete { ops, choices }),
            Err(_) if context.need.is_some() => Ok(MentionStep::Need(context.need.unwrap())),
            Err(error) => Err(error),
        }
    }
}
fn resolve(
    plan: &MentionPlan,
    context: &mut Context<'_>,
) -> Result<(Vec<Value>, Vec<MentionChoice>)> {
    let mut choices = Vec::new();
    let mut copied_bytes = 0usize;
    for (index, candidate) in plan.candidates.iter().enumerate() {
        let lookup = MentionLookup {
            kind: candidate.kind,
            query: candidate.query.clone(),
        };
        let mut matches = context
            .observed
            .searches
            .iter()
            .filter(|v| v.lookup == lookup);
        let Some(search) = matches.next() else {
            return context.missing(MentionNeed::Search(lookup));
        };
        if matches.next().is_some() || search.rows.len() > 25 {
            return Err(SeedUnavailable::InvalidObservation);
        }
        let mut ids = BTreeSet::new();
        for row in &search.rows {
            if !ids.insert(id(row)?) {
                return Err(SeedUnavailable::InvalidObservation);
            }
            let size = serde_json::to_vec(row)
                .map_err(|_| SeedUnavailable::MalformedMessage)?
                .len();
            copied_bytes = copied_bytes
                .checked_add(size)
                .filter(|n| *n <= 8 * 1024 * 1024)
                .ok_or(SeedUnavailable::Limit)?;
        }
        let rows = search.rows.clone();
        let selected = match candidate.kind {
            MentionKind::Member => member::select(candidate, &rows, context)?,
            MentionKind::Channel => {
                // rNCs postfilters the actual ordered results by exact hfg5 name.
                let mut eligible = Vec::new();
                for row in &rows {
                    if flag(row, "is_im")? {
                        continue;
                    }
                    let name = row
                        .get("name")
                        .and_then(Value::as_str)
                        .ok_or(SeedUnavailable::InvalidObservation)?;
                    if name == candidate.query {
                        eligible.push(row);
                    }
                }
                let mut local = None;
                for row in &eligible {
                    if !flag(row, "fromAnotherTeam")? {
                        local = Some(*row);
                        break;
                    }
                }
                local.or_else(|| eligible.first().copied()).map(|row| {
                    (
                        row.clone(),
                        format!("#{}", row["name"].as_str().unwrap()),
                        candidate.match_length_utf16,
                    )
                })
            }
        };
        if let Some((raw, label, length)) = selected {
            let id = id(&raw)?.to_owned();
            if id != "UNVERIFIED" {
                choices.push(MentionChoice {
                    candidate_index: index,
                    raw,
                    id,
                    label,
                    global_index_utf16: candidate.global_index_utf16,
                    length_utf16: length,
                });
            }
        }
    }
    if choices.is_empty() {
        return Ok((plan.original.clone(), choices));
    }
    let Some(words) = context.observed.highlight_words.as_ref() else {
        return context.missing(MentionNeed::Highlights);
    };
    // fhSo branches on emoji_displayed_as_text only for emoji candidates.
    // rNCs returns [] for those before lookup; admitted members/channels use
    // the same original slackmention splice in either observed display mode.
    choices.sort_by_key(|v| v.global_index_utf16);
    let mut document = Document::capture(&plan.original)?;
    let mut changes = Change::default();
    let mut end = 0;
    for choice in &choices {
        let next = choice
            .global_index_utf16
            .checked_add(choice.length_utf16)
            .ok_or(SeedUnavailable::Limit)?;
        // Original source uses original offsets. Overlap/invalid spans remain
        // an explicit refusal; do not repair positions or silently omit a choice.
        if choice.global_index_utf16 < end || next > document.len() || choice.length_utf16 == 0 {
            return Err(SeedUnavailable::InvalidObservation);
        }
        changes.retain(choice.global_index_utf16 - end);
        changes.delete(choice.length_utf16);
        let user = format!("<@{}", choice.id.strip_prefix('@').unwrap_or(&choice.id));
        let group = format!("<!subteam^{}", choice.id);
        let highlighted = matches!(
            choice.id.as_str(),
            "BKhere"
                | "BKchannel"
                | "BKeveryone"
                | "BKgroup"
                | "BKsf:accountteam"
                | "BKsf:dealteam"
                | "BKassistant:einstein"
        ) || words.iter().any(|word| word == &user || word == &group);
        let attrs = json!({"slackmention":{"id":choice.id,"label":choice.label,"mention":highlighted,"unverified":false}});
        let insert = Document::capture(&[json!({"insert":choice.label,"attributes":attrs})])?;
        changes.insert_document(insert);
        end = next;
    }
    document.apply(&changes)?;
    Ok((document.ops()?, choices))
}
fn id(row: &Value) -> Result<&str> {
    row.get("id")
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .ok_or(SeedUnavailable::InvalidObservation)
}
fn flag(row: &Value, key: &str) -> Result<bool> {
    match row.get(key) {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(v)) => Ok(*v),
        _ => Err(SeedUnavailable::InvalidObservation),
    }
}
