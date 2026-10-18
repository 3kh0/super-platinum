//! 2YmW file matching under Profile's onlySlackNative/allowEmpty/preferExact.
use super::file_title_source::CandidateObservation;
use crate::block_actions::home::workflow::{
    directory_local::{FuzzyMatcher, FuzzyOptions, PreparedQuery},
    directory_ranking::Error,
};
use serde_json::Value;

/// Actual tpNE.k, DZwJ.q and 7MjS.Ss observations, required only for canned
/// templates. Creator enterprise is the actual 7MjS.Ss result (including its
/// enterprise-record feature branch), not a copied field or guessed mapping.
pub struct TemplateContext {
    pub current_team: String,
    pub enterprise: bool,
    pub paid: bool,
    pub creator_enterprise: Option<String>,
}
fn truth(v: &Value) -> bool {
    match v {
        Value::Null | Value::Bool(false) => false,
        Value::String(s) => !s.is_empty(),
        Value::Number(n) => n.as_f64().is_some_and(|n| n != 0.0),
        _ => true,
    }
}
fn text(value: &Value) -> Result<&str, Error> {
    match value {
        Value::Null => Ok(""),
        Value::String(s) => Ok(s),
        _ => Err(Error::InvalidInput),
    }
}

/// Raw/effective selector predicates; no typed-browser fallback or extra
/// writable/template exclusion. Template history may be unavailable upstream.
pub fn allows(
    raw: &Value,
    template: impl FnOnce() -> Result<TemplateContext, Error>,
) -> Result<bool, Error> {
    if !raw.is_object() {
        return Err(Error::InvalidInput);
    }
    if truth(&raw["isUnknown"])
        || truth(&raw["isNonExistent"])
        || truth(&raw["is_tombstoned"])
        || truth(&raw["is_ai_suggested"])
    {
        return Ok(false);
    }
    if ["not_visible", "template_not_visible"]
        .iter()
        .any(|v| raw["mode"] == *v || raw["file_access"] == *v)
    {
        return Ok(false);
    }
    let canvas = raw["mode"] == "quip" || raw["filetype"] == "quip";
    let list = (raw["mode"] == "list" || raw["filetype"] == "list") && raw["subtype"] != "poll";
    if !canvas && !list {
        return Ok(false);
    }
    if canvas && truth(&raw["canvas_template_mode"]) && raw["user"] == "USLACKBOT" {
        let context = template()?;
        if context.current_team.is_empty() {
            return Err(Error::MissingObservation("current file team".into()));
        }
        let creator = context
            .creator_enterprise
            .as_deref()
            .unwrap_or(text(&raw["user_team"])?);
        if creator != context.current_team || (!context.enterprise && !context.paid) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Actual hNDQ values captured in the renderer; no locale guess in pure code.
pub struct Names {
    values: [String; 4],
    scored: [bool; 4],
}
impl Names {
    pub fn observe(
        raw: &Value,
        normalize: impl Fn(&str) -> Result<String, Error>,
    ) -> Result<Self, Error> {
        let inputs = [
            text(&raw["name"])?,
            text(&raw["title"])?,
            text(&raw["external_url"])?,
            text(&raw["permalink"])?,
        ];
        let values = [
            normalize(inputs[0])?,
            normalize(inputs[1])?,
            normalize(inputs[2])?,
            normalize(inputs[3])?,
        ];
        Ok(Self {
            values,
            scored: [
                !inputs[0].is_empty(),
                !inputs[1].is_empty() && inputs[1] != inputs[0],
                !inputs[2].is_empty(),
                !inputs[3].is_empty(),
            ],
        })
    }
}
pub struct FileScored<'a> {
    pub snapshot: &'a CandidateObservation,
    pub id: &'a str,
    pub score: u32,
    pub is_exact_match: Option<bool>,
}
fn score(
    query: &PreparedQuery,
    names: Names,
    query_has_link: bool,
    ignore_fuzzy: bool,
) -> Option<(u32, bool)> {
    let matcher = FuzzyMatcher::new(&query.matcher_normalized, FuzzyOptions::default());
    let score = names
        .values
        .iter()
        .enumerate()
        .filter(|(i, _)| names.scored[*i] && (*i < 2 || query_has_link))
        .filter_map(|(_, s)| {
            if s == &query.matcher_normalized {
                Some(0)
            } else {
                matcher.score(s)
            }
        })
        .min()
        .filter(|n| *n <= 3)?;
    Some((
        if ignore_fuzzy { 0 } else { score },
        names.values.iter().any(|v| v == &query.normalized),
    ))
}
pub fn matches<'a>(
    query: &PreparedQuery,
    rows: &'a [CandidateObservation],
    query_has_link: bool,
    ignore_fuzzy: bool,
    template: impl Fn(&CandidateObservation) -> Result<TemplateContext, Error>,
    names: impl Fn(&CandidateObservation) -> Result<Names, Error>,
) -> Result<Vec<FileScored<'a>>, Error> {
    let mut out = Vec::new();
    for row in rows {
        if !allows(row.effective(), || template(row))? {
            continue;
        }
        let (score, is_exact_match) = if query.normalized.is_empty() {
            (0, None)
        } else {
            let Some((score, exact)) = score(query, names(row)?, query_has_link, ignore_fuzzy)
            else {
                continue;
            };
            (score, Some(exact))
        };
        out.push(FileScored {
            snapshot: row,
            id: row.id(),
            score,
            is_exact_match,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
