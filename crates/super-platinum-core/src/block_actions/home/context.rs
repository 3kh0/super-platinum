//! Pure +aGg/UONW Home context selection. Callers supply independently observed
//! inputs; this module neither acquires metadata nor turns cache data into authority.
use crate::slack::Error;
use serde_json::Value;
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Selection {
    NonUnified,
    /// A known empty candidate set legitimately yields no context. The Home
    /// request then uses its installed team, unlike missing observations (Err).
    Unified {
        team_id: Option<String>,
    },
}
impl Selection {
    pub fn team_id(&self) -> Option<&str> {
        match self {
            Self::NonUnified => None,
            Self::Unified { team_id } => team_id.as_deref(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Inputs<'a> {
    /// None is unobserved, never an inferred false.
    pub unified: Option<bool>,
    pub channel_override: Option<&'a str>,
    /// None is unobserved; Some([]) is an observed empty collection.
    pub internal_workspace_ids: Option<&'a [String]>,
    /// An observed missing enterprise teams field (or no bot channel) is None.
    /// A caller must not use None to disguise a failed member acquisition.
    pub bot_enterprise_teams: Option<&'a [String]>,
    pub active_team_id: Option<&'a str>,
    /// Exact normalized-store value; no JSON parsing or string coercion here.
    pub workspace_filter_selected: Option<&'a Value>,
    pub default_workspace_id: Option<&'a str>,
}

fn invalid() -> Error {
    Error::Api("app_home_context_unavailable".into())
}
pub(super) fn validate_identity(value: &str) -> Result<(), Error> {
    if value.is_empty()
        || value.len() > 255
        || value.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        Err(invalid())
    } else {
        Ok(())
    }
}

/// Original ordered intersection; an absent/empty intersection falls back to
/// all ordered unique internal IDs. Bot team order never reorders the menu.
pub fn candidates(
    internal_workspace_ids: &[String],
    bot_enterprise_teams: Option<&[String]>,
) -> Result<Vec<String>, Error> {
    let mut seen = HashSet::new();
    let mut internal = Vec::new();
    for id in internal_workspace_ids {
        validate_identity(id)?;
        if seen.insert(id.as_str()) {
            internal.push(id.clone());
        }
    }
    let Some(bot) = bot_enterprise_teams else {
        return Ok(internal);
    };
    let bot: HashSet<_> = bot.iter().map(String::as_str).collect();
    let intersection: Vec<_> = internal
        .iter()
        .filter(|id| bot.contains(id.as_str()))
        .cloned()
        .collect();
    Ok(if intersection.is_empty() {
        internal
    } else {
        intersection
    })
}

/// Original active-self-team predicate is stricter than general route IDs.
fn active_team(value: &str) -> bool {
    value.len() >= 9
        && value.starts_with('T')
        && value[1..]
            .bytes()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

pub fn select(input: &Inputs<'_>) -> Result<Selection, Error> {
    match input.unified {
        None => return Err(invalid()),
        Some(false) => return Ok(Selection::NonUnified),
        Some(true) => {}
    }
    // The original override wins even outside current candidate membership.
    if let Some(team) = input.channel_override.filter(|s| !s.is_empty()) {
        validate_identity(team)?;
        return Ok(Selection::Unified {
            team_id: Some(team.into()),
        });
    }
    let internal = input.internal_workspace_ids.ok_or_else(invalid)?;
    let possible = candidates(internal, input.bot_enterprise_teams)?;
    let includes = |id: &&str| possible.iter().any(|candidate| candidate == *id);
    let active = input.active_team_id.filter(|id| active_team(id));
    // UONW treats a nonstring or a string outside self IDs as the All filter.
    let selected = input
        .workspace_filter_selected
        .and_then(Value::as_str)
        .filter(|id| *id != "all")
        .filter(|id| internal.iter().any(|candidate| candidate.as_str() == *id));
    let team = active
        .filter(includes)
        .or_else(|| selected.filter(includes))
        .or_else(|| input.default_workspace_id.filter(includes))
        .or_else(|| possible.first().map(String::as_str));
    Ok(Selection::Unified {
        team_id: team.map(str::to_owned),
    })
}

#[cfg(test)]
mod tests;
