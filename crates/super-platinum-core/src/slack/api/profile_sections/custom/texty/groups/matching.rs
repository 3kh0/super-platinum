use super::*;
use crate::block_actions::home::workflow::directory_local::{
    FuzzyMatcher, FuzzyOptions, PreparedQuery,
};
/// Renderer-owned locale lowering and hNDQ normalization of the *effective*
/// group. Scoring the stored alias differs from exact matching the actual name.
pub struct Names {
    scoring_name: String,
    scoring_handle: String,
    exact_name: String,
    exact_handle: String,
}
impl Names {
    pub fn observe(
        row: &Snapshot,
        locale_lower: impl Fn(&str) -> Result<String, Error>,
        normalize: impl Fn(&str) -> Result<String, Error>,
    ) -> Result<Self, Error> {
        let raw = row.effective();
        let name = string(&raw["name"])?;
        let stored = string(&raw["_name_lc"])?;
        let scoring = if stored.is_empty() {
            locale_lower(name)?
        } else {
            stored.to_owned()
        };
        let handle = string(&raw["handle"])?;
        let stripped = handle.strip_prefix('@').unwrap_or(handle);
        // XXup strips one prefix only; a lone @ has empty fuzzy handle but a
        // prefixed exact-match input of @. No trim or invented fallback label.
        let prefixed = if handle.is_empty() {
            String::new()
        } else {
            format!("@{stripped}")
        };
        Ok(Self {
            scoring_name: normalize(&scoring)?,
            scoring_handle: if stripped.is_empty() {
                String::new()
            } else {
                normalize(stripped)?
            },
            exact_name: normalize(name)?,
            exact_handle: normalize(&prefixed)?,
        })
    }
}
pub struct GroupScored<'a> {
    pub snapshot: &'a Snapshot,
    pub id: &'a str,
    pub score: u32,
    pub is_exact_match: Option<bool>,
}
impl GroupScored<'_> {
    pub fn item(&self) -> &Value {
        self.snapshot.effective()
    }
}
/// 2YmW.filterUserGroups, fixed aD6p/fRHe options. Raw order and duplicate
/// occurrences survive. Groups have no remote fuzzy pass-through branch.
pub fn matches<'a>(
    query: &PreparedQuery,
    rows: &'a [Snapshot],
    eligible: impl Fn(&Snapshot) -> Result<bool, Error>,
    names: impl Fn(&Snapshot) -> Result<Names, Error>,
) -> Result<Vec<GroupScored<'a>>, Error> {
    let matcher = FuzzyMatcher::new(&query.matcher_normalized, FuzzyOptions::default());
    let mut out = Vec::new();
    for row in rows {
        let raw = row.effective();
        if truth(&raw["isUnknown"]) || truth(&raw["isNonExistent"]) || !eligible(row)? {
            continue;
        }
        if query.normalized.is_empty() {
            out.push(GroupScored {
                snapshot: row,
                id: row.id(),
                score: 0,
                is_exact_match: None,
            });
            continue;
        }
        let names = names(row)?;
        let score = matcher
            .score(&names.scoring_name)
            .into_iter()
            .chain(matcher.score(&names.scoring_handle))
            .min();
        if let Some(score) = score.filter(|n| *n <= 3) {
            out.push(GroupScored {
                snapshot: row,
                id: row.id(),
                score,
                is_exact_match: Some(
                    names.exact_name == query.normalized || names.exact_handle == query.normalized,
                ),
            });
        }
    }
    Ok(out)
}
