//! Supplied native builtin-name model using Source-qualified Core scanning.
//! This does not assert Source worker, locale, frecency or final-order equality.
use super::*;
use std::{cmp::Ordering, collections::BTreeMap};
use super_platinum_core::composer_emoji::{
    catalog::Catalog,
    keyword,
    search::{self, Entry, Name},
};
struct Group {
    ordinal: usize,
    choices: BTreeMap<String, usize>,
}
#[cfg(all(test, feature = "protected-native-fixture"))]
mod tests;
pub(super) struct Model {
    entries: Vec<Entry>,
    groups: BTreeMap<String, Group>,
}
fn object_index(name: &str) -> Option<u32> {
    let n = name.parse::<u32>().ok()?;
    (n != u32::MAX && n.to_string() == name).then_some(n)
}
fn query_space(c: char) -> bool {
    matches!(c, '\u{0009}'..='\u{000d}'|'\u{0020}'|'\u{00a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}')
}
impl Model {
    /// Build once after positive Core projections, off render. Full target-key
    /// consolidation precedes restriction; no alphabetic order is called Source.
    pub(super) fn build(catalog: &Catalog, choices: &[Selected]) -> Self {
        let mut canonical = BTreeMap::new();
        let mut aliases = BTreeMap::new();
        for (index, choice) in choices.iter().enumerate() {
            match choice {
                Selected::Canonical(c) => {
                    canonical.insert(c.base(), index);
                }
                Selected::Alias(c) => {
                    aliases.insert(c.matched_alias(), (c.canonical_group(), index));
                }
                Selected::Custom(_) => {}
            }
        }
        let mut ordered = catalog
            .standard
            .entries
            .iter()
            .enumerate()
            .collect::<Vec<_>>();
        ordered.sort_by(
            |(ai, a), (bi, b)| match (object_index(&a.name), object_index(&b.name)) {
                (Some(a), Some(b)) => a.cmp(&b),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => ai.cmp(bi),
            },
        );
        let mut entries = Vec::<Entry>::new();
        let mut groups = BTreeMap::<String, Group>::new();
        for (_, record) in ordered {
            let target = record.alias_of.as_deref().unwrap_or(&record.name);
            let Some(&primary) = canonical.get(target) else {
                continue;
            };
            if !groups.contains_key(target) {
                let ordinal = entries.len();
                entries.push(Entry {
                    id: target.into(),
                    is_emoji: false,
                    primary: Name {
                        canonical: target.into(),
                        display: None,
                    },
                    aliases: vec![],
                    keywords: vec![],
                });
                groups.insert(
                    target.into(),
                    Group {
                        ordinal,
                        choices: BTreeMap::from([(target.into(), primary)]),
                    },
                );
            }
            if record.alias_of.is_some() {
                if let Some(&(group, index)) = aliases.get(record.name.as_str()) {
                    if group == target {
                        let model = groups.get_mut(target).unwrap();
                        entries[model.ordinal].aliases.push(Name {
                            canonical: record.name.clone(),
                            display: None,
                        });
                        model.choices.insert(record.name.clone(), index);
                    }
                }
            }
        }
        Self { entries, groups }
    }
    /// Explicit native direct-name model, with no warm aliases or history.
    pub(super) fn direct(names: impl IntoIterator<Item = (String, usize)>) -> Self {
        let mut entries = Vec::new();
        let mut groups = BTreeMap::new();
        for (name, index) in names {
            if groups.contains_key(&name) {
                continue;
            }
            let ordinal = entries.len();
            entries.push(Entry {
                id: name.clone(),
                is_emoji: false,
                primary: Name {
                    canonical: name.clone(),
                    display: None,
                },
                aliases: vec![],
                keywords: vec![],
            });
            groups.insert(
                name.clone(),
                Group {
                    ordinal,
                    choices: BTreeMap::from([(name, index)]),
                },
            );
        }
        Self { entries, groups }
    }
    pub(super) fn empty(literal: &str) -> bool {
        literal.len() <= 4096
            && literal
                .replace(':', "")
                .trim_matches(query_space)
                .is_empty()
    }
    pub(super) fn query(&self, literal: &str) -> Option<Vec<usize>> {
        if literal.len() > 4096 {
            return None;
        }
        let removed = literal.replace(':', "");
        let query = removed.trim_matches(query_space);
        // Finite native name-input profile. The Core alias parser stays exact;
        // Source leading-plus, Unicode letters/localization and other modes
        // remain outside this supplied model. Core matching folds ASCII case
        // and collapses these exact ECMAScript whitespace characters.
        if query.is_empty()
            || query.len() > 256
            || !query
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphanumeric)
            || !query
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-') || query_space(c))
        {
            return None;
        }
        let mut candidates = search::candidates(query, &self.entries, keyword::Options::default())
            .into_iter()
            .filter_map(|candidate| {
                let group = self.groups.get(&candidate.id)?;
                if candidate.name != candidate.id {
                    return None;
                }
                let score = candidate.name_score.filter(|s| s.is_finite())?;
                let index = *group.choices.get(&candidate.canonical)?;
                Some((score, group.ordinal, index))
            })
            .collect::<Vec<_>>();
        // Explicit native cross-group policy. Do not call a zero-history or
        // remote ranker, default worker flags, or invent missing history.
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        Some(candidates.into_iter().take(250).map(|c| c.2).collect())
    }
}
