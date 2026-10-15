//! Worker-owned ranking plus serial bridge admission. Directory completion keeps
//! its existing path; the emoji worker receives only immutable captured inputs.
use crate::emoji_catalog::Loaded;
use std::sync::Arc;
use super_platinum_core::composer_emoji::{
    self as emoji,
    admission::{self, Snapshot},
    history::History,
    history_ranking,
    insertion::Representation,
    keyword,
    model::{Metadata, Model},
    preferences::Preferences,
    ranking::SimilarityCache,
};

#[derive(Clone)]
pub(super) struct Query {
    pub seq: u64,
    pub owner: String,
    pub snapshot: Snapshot,
    pub loaded: Arc<Loaded>,
    pub preferences: Preferences,
    pub japanese: bool,
    pub now_ms: f64,
}

#[derive(Clone, serde::Serialize)]
pub(super) struct Row {
    pub id: String,
    pub label: String,
    pub detail: String,
}

#[derive(serde::Serialize)]
pub(super) struct Results {
    pub kind: &'static str,
    pub seq: u64,
    pub owner: String,
    pub catalog: String,
    pub preferences: String,
    pub choices: Vec<Row>,
    pub status: &'static str,
}

pub(super) struct Calculated {
    pub query: Query,
    pub results: Results,
    pub choices: Vec<admission::Choice>,
}

pub(super) fn trigger(snapshot: &Snapshot, japanese: bool) -> Option<emoji::Trigger> {
    let (anchor, focus) = snapshot.byte_selection()?;
    emoji::Trigger::at(
        &snapshot.text,
        anchor,
        focus,
        emoji::Options {
            japanese,
            user_solicited: false,
        },
    )
}

/// One instance belongs to one mounted completion worker. It is moved through
/// spawn_blocking, never locked from a render/event path. Replacement catalog
/// allocations reset the observable source cosine cache.
pub(super) struct Worker {
    catalog: Option<Arc<Loaded>>,
    #[cfg(feature = "emoji-retirement-observation")]
    trace_state: u8,
    #[cfg(feature = "emoji-retirement-observation")]
    trace_mount: u64,
    cache: SimilarityCache,
    model: Model,
    metadata: Metadata,
    preference_revision: Option<u64>,
    history: Result<History, String>,
}

impl Default for Worker {
    fn default() -> Self {
        Self {
            catalog: None,
            #[cfg(feature = "emoji-retirement-observation")]
            trace_state: 0,
            #[cfg(feature = "emoji-retirement-observation")]
            trace_mount: 0,
            cache: SimilarityCache::default(),
            model: Model::fallback(),
            metadata: Metadata::default(),
            preference_revision: None,
            history: Ok(History::default()),
        }
    }
}

impl Worker {
    pub fn waiting(&mut self) {
        #[cfg(feature = "emoji-retirement-observation")]
        {
            self.trace_state = 1;
            if let Some(loaded) = &self.catalog {
                crate::performance::emoji_retirement::worker_wait(
                    crate::performance::emoji_retirement::payload_id(&*loaded.catalog),
                    self.trace_mount,
                );
            }
        }
    }
    pub fn calculate(&mut self, query: Query) -> Calculated {
        crate::performance::emoji_retirement::worker_active();
        let mount = crate::performance::emoji_retirement::scalar_id(&query.owner);
        #[cfg(feature = "emoji-retirement-observation")]
        {
            self.trace_state = 2;
            self.trace_mount = mount;
        }
        if self
            .catalog
            .as_ref()
            .is_none_or(|old| !Arc::ptr_eq(old, &query.loaded))
        {
            self.cache.clear();
            self.preference_revision = None;
            crate::performance::emoji_retirement::synchronous(8, 2, mount, || {
                self.catalog = Some(query.loaded.clone());
            });
        }
        if self.preference_revision != Some(query.preferences.revision()) {
            self.preference_revision = Some(query.preferences.revision());
            self.history = match query.preferences.frecency() {
                Some(value) => History::from_pref(value),
                None => Ok(History::default()),
            };
        }
        let mut result = Calculated {
            results: Results {
                kind: "emoji_results",
                seq: query.seq,
                owner: query.owner.clone(),
                catalog: query.loaded.token.clone(),
                preferences: query.preferences.revision().to_string(),
                choices: vec![],
                status: "",
            },
            choices: vec![],
            query,
        };
        let query = &result.query;
        let Some(trigger) = trigger(&query.snapshot, query.japanese) else {
            return result;
        };
        if !query.preferences.loaded() {
            result.results.status = "Loading emoji…";
            return result;
        }
        let Ok(history) = &self.history else {
            result.results.status = "Emoji suggestions unavailable";
            return result;
        };
        let candidates = emoji::search::candidates(
            &trigger.query,
            &query.loaded.catalog.search_entries,
            keyword::Options::default(),
        );
        let context = history_ranking::Context {
            history,
            now_ms: query.now_ms,
            model: &self.model,
            metadata: &self.metadata,
            source: "texty-autocomplete",
            exact_boost: false,
        };
        let ranked = history_ranking::rank(
            &trigger.query,
            candidates,
            25,
            &context,
            &mut self.cache,
            super_platinum_core::state::sidebar_name_cmp,
        );
        let Ok(ranked) = ranked else {
            result.results.status = "Emoji suggestions unavailable";
            return result;
        };
        let representation = if query.preferences.as_text()
            || emoji::context::in_markdown_code(&query.snapshot.text[..trigger.start])
        {
            Representation::Shortcode
        } else {
            Representation::Rendered
        };
        let spacer = if query.snapshot.text[trigger.end..]
            .chars()
            .next()
            .is_some_and(|c| c.is_whitespace() || ",.;:!?)]}".contains(c))
        {
            ""
        } else {
            " "
        };
        for ranked in ranked {
            let matched = &ranked.candidate.canonical;
            let Some(resolved) = query
                .loaded
                .catalog
                .resolve(matched, query.preferences.preferred_tone())
            else {
                continue;
            };
            let id = serde_json::to_string(&(&ranked.candidate.id, matched)).expect("string tuple");
            let value = resolved.insertion(representation);
            result.results.choices.push(Row {
                id: id.clone(),
                label: resolved
                    .glyph
                    .clone()
                    .map(|glyph| format!("{glyph} {}", resolved.shortcode))
                    .unwrap_or_else(|| resolved.shortcode.clone()),
                detail: String::new(),
            });
            result.choices.push(admission::Choice {
                id,
                value,
                start: query.snapshot.text[..trigger.start].encode_utf16().count(),
                end: query.snapshot.focus,
                spacer: spacer.into(),
                usage_id: resolved.usage_id,
                query: trigger.query.clone(),
            });
        }
        if result.results.choices.is_empty() {
            result.results.status = "No matches";
        }
        result
    }
}

#[cfg(feature = "emoji-retirement-observation")]
impl Drop for Worker {
    fn drop(&mut self) {
        let context = match self.trace_state {
            1 => 9,
            2 => 10,
            _ => 0,
        };
        let _release = crate::performance::emoji_retirement::scope(context, 0, self.trace_mount);
        // catalog is already the first declared field; preserve destruction order.
        drop(self.catalog.take());
    }
}
