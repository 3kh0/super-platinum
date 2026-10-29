//! Staged native known-identity query editing; parsing grants no permissions.
//! Callers must supply/revalidate eligible current canonical records. Name/locale
//! lookup and Source provider/sentinel history are deliberately not inferred.
use super::SearchFilters;
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
};
mod fields;
mod lex;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scope {
    Channels,
    Authors,
}
impl Scope {
    fn index(self) -> usize {
        match self {
            Self::Channels => 0,
            Self::Authors => 1,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeChoice {
    kind: Scope,
    id: String,
    label: String,
}
impl ScopeChoice {
    pub fn channel(id: impl Into<String>, name: impl Into<String>) -> Option<Self> {
        let id = id.into();
        channel_id(&id).then(|| Self {
            kind: Scope::Channels,
            id,
            label: name.into(),
        })
    }
    pub fn author(id: impl Into<String>, name: impl Into<String>) -> Option<Self> {
        let id = id.into();
        author_id(&id).then(|| Self {
            kind: Scope::Authors,
            id,
            label: name.into(),
        })
    }
    pub fn kind(&self) -> Scope {
        self.kind
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn label(&self) -> &str {
        &self.label
    }
    fn clause(&self) -> Result<String, &'static str> {
        match self.kind {
            Scope::Authors => Ok(format!("from:<@{}>", self.id)),
            Scope::Channels => {
                // Retained references never pass through this formatter. This
                // refusal bounds NEW native clauses rather than rewriting names.
                if self.label.contains(['"', '\\', '\n', '\r', '<']) {
                    return Err("This channel name cannot be added as a search filter.");
                }
                let name = self.label.replace('>', "");
                Ok(if name.is_empty() {
                    format!("in:<#{}>", self.id)
                } else {
                    format!("in:<#{}|{name}>", self.id)
                })
            }
        }
    }
}
fn prefixed_id(id: &str, prefixes: &[u8]) -> bool {
    id.len() >= 9
        && prefixes.contains(&id.as_bytes()[0])
        && id.as_bytes()[1..]
            .iter()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}
fn channel_id(id: &str) -> bool {
    prefixed_id(id, b"CGDZ")
}
fn author_id(id: &str) -> bool {
    prefixed_id(id, b"UW") || matches!(id, "USLACKBOT" | "USEARCH" | "UCHANNELEXPERT" | "USLACK")
}
fn identity(token: &str) -> Option<(Scope, &str)> {
    if let Some(value) = token
        .strip_prefix("in:<#")
        .and_then(|s| s.strip_suffix('>'))
    {
        let (id, label) = value
            .split_once('|')
            .map(|(a, b)| (a, Some(b)))
            .unwrap_or((value, None));
        if !channel_id(id)
            || label.is_some_and(|s| s.is_empty() || s.contains(['<', '>', '"', '\\', '\n', '\r']))
        {
            return None;
        }
        return Some((Scope::Channels, id));
    }
    let id = token
        .strip_prefix("from:<@")
        .and_then(|s| s.strip_suffix('>'))?;
    author_id(id).then_some((Scope::Authors, id))
}
#[derive(Debug, Clone)]
struct Owned {
    kind: Scope,
    id: String,
    range: Range<usize>,
}
#[derive(Debug, Clone)]
pub struct ScopeEdit {
    original: String,
    lex: lex::Lex,
    choices: [HashMap<String, ScopeChoice>; 2],
    order: [Vec<String>; 2],
    baseline: [HashSet<String>; 2],
    selected: [HashSet<String>; 2],
    selected_order: [Vec<String>; 2],
    owned: Vec<Owned>,
    fields: SearchFilters,
}
impl ScopeEdit {
    pub fn capture(query: &str, choices: Vec<ScopeChoice>) -> Result<Self, &'static str> {
        let mut maps: [HashMap<String, ScopeChoice>; 2] = [HashMap::new(), HashMap::new()];
        let mut order = [Vec::new(), Vec::new()];
        for choice in choices {
            let index = choice.kind.index();
            let id = choice.id.clone();
            if maps[index].insert(id.clone(), choice).is_some() {
                return Err("A search choice has a duplicate identity.");
            }
            order[index].push(id);
        }
        let lex = lex::scan(query);
        let mut selected: [HashSet<String>; 2] = [HashSet::new(), HashSet::new()];
        let mut selected_order = [Vec::new(), Vec::new()];
        let mut owned = Vec::new();
        for range in &lex.tokens {
            if let Some((kind, id)) = identity(&query[range.clone()]) {
                let index = kind.index();
                if !maps[index].contains_key(id) {
                    continue;
                }
                if selected[index].insert(id.into()) {
                    selected_order[index].push(id.into());
                }
                owned.push(Owned {
                    kind,
                    id: id.into(),
                    range: range.clone(),
                });
            }
        }
        let fields = fields::project(query, &lex);
        Ok(Self {
            original: query.into(),
            lex,
            choices: maps,
            order,
            baseline: selected.clone(),
            selected,
            selected_order,
            owned,
            fields,
        })
    }
    pub fn fields(&self) -> SearchFilters {
        self.fields.clone()
    }
    pub fn choices(&self, kind: Scope) -> impl Iterator<Item = &ScopeChoice> {
        let index = kind.index();
        self.order[index]
            .iter()
            .map(move |id| &self.choices[index][id])
    }
    pub fn selected(&self, kind: Scope, id: &str) -> bool {
        self.selected[kind.index()].contains(id)
    }
    pub fn selected_count(&self, kind: Scope) -> usize {
        self.selected[kind.index()].len()
    }
    pub fn set(&mut self, kind: Scope, id: &str, value: bool) -> Result<(), &'static str> {
        let index = kind.index();
        if !self.choices[index].contains_key(id) {
            return Err("Choose a current known channel or author.");
        }
        if value {
            if self.selected[index].insert(id.into()) {
                self.selected_order[index].push(id.into());
            }
        } else {
            self.selected[index].remove(id);
            self.selected_order[index].retain(|v| v != id);
        }
        Ok(())
    }
    pub fn clear(&mut self, kind: Scope) {
        let index = kind.index();
        self.selected[index].clear();
        self.selected_order[index].clear();
    }
    pub fn apply(&self, query: &str, fields: &SearchFilters) -> Result<String, &'static str> {
        if query != self.original {
            return Err("Search changed. Reopen the filters and try again.");
        }
        let (mut remove, other_append) = fields::edits(query, &self.lex, &self.fields, fields)?;
        let mut append = Vec::new();
        for kind in [Scope::Channels, Scope::Authors] {
            let index = kind.index();
            for id in &self.selected_order[index] {
                if !self.baseline[index].contains(id) {
                    append.push(self.choices[index][id].clause()?);
                }
            }
        }
        append.extend(other_append);
        if !append.is_empty() {
            if self.lex.unfinished_quote || super::unfinished_quote(query) {
                return Err("Close the quoted phrase before adding a filter.");
            }
            if self.lex.incomplete_reference {
                return Err("Close the reference before adding a filter.");
            }
        }
        remove.extend(
            self.owned
                .iter()
                .filter(|v| !self.selected[v.kind.index()].contains(&v.id))
                .map(|v| v.range.clone()),
        );
        if remove.is_empty() && append.is_empty() {
            return Ok(query.into());
        }
        remove.sort_by_key(|r| (r.start, r.end));
        remove.dedup();
        let mut result = query.to_owned();
        for range in remove.into_iter().rev() {
            result.replace_range(range, "");
        }
        for clause in append {
            if result.chars().last().is_some_and(|ch| !ch.is_whitespace()) {
                result.push(' ');
            }
            result.push_str(&clause);
        }
        Ok(result)
    }
}
#[cfg(test)]
mod tests;
