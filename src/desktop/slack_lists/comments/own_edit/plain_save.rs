//! Ordinary plain Save preparation under the original mounted edit and policy.
//! This is not admission, a lease, or an automatic-mention search substitute.
use super::*;
use crate::directory_member_fetches::Context;
use crate::profile_edit::sections::rich::search::file_source::FileObservation;
use super_platinum_core::slack_lists::comments::edit_seed::prepare::{
    EncodingNeed, EncodingObservations, EncodingStep, PreparedPlain,
};
mod custom;
mod emoji;
mod mentions;
mod processing;
#[cfg(test)]
mod tests;

pub(super) struct Captured<'a> {
    facts: Facts,
    _custom: Option<Arc<custom::Complete>>,
    policy: &'a own_edit_policy::Observation,
    pub(super) prepared: PreparedPlain,
    pub(super) headers: bool,
    pub(super) encoding: EncodingObservations,
    /// Unavailable qszY.Kr facts disable comparison. The Save caller may supply
    /// observed absence only from its fresh native no-attachment-effects
    /// capability and independently admitted attachmentless original source.
    pub(super) unfurled: Option<Value>,
}
impl Captured<'_> {
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        self.facts.current(shell) && self.policy.current(shell)
    }
    pub(super) fn returning_current(&self, shell: &ShellState) -> bool {
        self.facts.current_with_finish(shell, true) && self.policy.current(shell)
    }
}
struct Facts {
    binding: Binding,
    context: Context,
    serial: u64,
    revision: u64,
    delta: PlainDelta,
    preferences_revision: u64,
    locale: String,
    files: Vec<FileObservation>,
    custom_revision: Option<u64>,
    mentions: mentions::Facts,
}
impl Facts {
    fn current(&self, shell: &ShellState) -> bool {
        self.current_with_finish(shell, false)
    }
    fn current_with_finish(&self, shell: &ShellState, finishing: bool) -> bool {
        self.binding.current(shell)
            && self.binding.source.owner.reading(shell, self.serial)
            && self.context.current(shell)
            && shell.core.active_team.as_deref() == Some(self.context.session.team_id.as_str())
            && self.preferences_revision
                == crate::app_home::autocomplete::preferences_revision(shell)
            && shell
                .core
                .workspaces
                .get(&self.context.session.team_id)
                .is_some_and(|w| {
                    w.locale.as_deref() == Some(self.locale.as_str())
                        && self
                            .custom_revision
                            .is_none_or(|revision| w.custom_emoji_revision == revision)
                })
            && session(shell, &self.binding).is_some_and(|s| {
                s.revision == self.revision
                    && s.blocks.is_empty()
                    && s.plain.as_ref() == Some(&self.delta)
                    && (finishing || !s.finishing)
            })
            && self.files.iter().all(|f| f.current(shell))
            && self.mentions.current(shell, &self.context)
    }
}
fn missing(part: &str) -> Error {
    Error::Api(format!("list_plain_save_{part}_unavailable"))
}

pub(super) async fn capture<'a>(
    state: Signal<ShellState>,
    binding: &Binding,
    serial: u64,
    policy: &'a own_edit_policy::Observation,
    delta: &PlainDelta,
) -> Result<Captured<'a>, Error> {
    delta.validate().map_err(|e| Error::Api(e.to_string()))?;
    let (mut facts, proof) = {
        let shell = state.peek();
        let s = session(&shell, binding).ok_or_else(|| missing("owner"))?;
        let context = Context::capture(&shell, binding.owner_team())?;
        let locale = shell
            .core
            .workspaces
            .get(binding.owner_team())
            .and_then(|w| w.locale.clone())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| missing("last_viewed_locale"))?;
        let facts = Facts {
            binding: binding.clone(),
            context,
            serial,
            revision: s.revision,
            delta: delta.clone(),
            preferences_revision: crate::app_home::autocomplete::preferences_revision(&shell),
            locale,
            files: Vec::new(),
            custom_revision: None,
            mentions: Default::default(),
        };
        if !facts.current(&shell) || !policy.current(&shell) {
            return Err(missing("owner"));
        }
        (facts, policy.proof())
    };
    let (settings, headers) = processing::settings(&proof)?;
    let user_locale = proof["userPreferences"]["prefs"]["locale"]
        .as_str()
        .map(str::to_owned);
    let mut standard = None;
    let mut custom: Option<Arc<custom::Complete>> = None;
    let mut mentions = super_platinum_core::slack_lists::comments::edit_seed::prepare::MentionObservations::default();
    let mut passes = 0;
    let prepared = loop {
        passes += 1;
        if passes > 256 {
            return Err(missing("observation_limit"));
        }
        let ops = delta.ops().to_vec();
        let locale = facts.locale.clone();
        let user_locale = user_locale.clone();
        let settings = settings.clone();
        let catalog = standard.clone();
        let complete = custom.clone();
        let observed = mentions.clone();
        let next = tokio::task::spawn_blocking(move || {
            processing::attempt_with_mentions(
                &ops,
                settings,
                &locale,
                user_locale.as_deref(),
                catalog.as_deref(),
                complete.as_deref(),
                &observed,
            )
        })
        .await
        .map_err(|_| missing("worker"))??;
        if !facts.current(&state.peek()) || !policy.current(&state.peek()) {
            return Err(missing("owner"));
        }
        match next {
            processing::Attempt::Ready(prepared) => break prepared,
            processing::Attempt::NeedStandard if standard.is_none() => {
                standard = Some(emoji::acquire(state, &facts, policy, &proof).await?);
            }
            processing::Attempt::NeedCustom if custom.is_none() => {
                custom = Some(acquire_custom(state, &mut facts, policy).await?);
            }
            processing::Attempt::NeedStandard | processing::Attempt::NeedCustom => {
                return Err(missing("catalog"));
            }
            processing::Attempt::NeedMention(need) => {
                mentions::fill(state, &mut facts, policy, &proof, need, &mut mentions).await?;
            }
        }
    };
    let mut encoding = EncodingObservations::default();
    for _ in 0..256 {
        if !facts.current(&state.peek()) || !policy.current(&state.peek()) {
            return Err(missing("owner"));
        }
        let plan = prepared
            .encoding(headers)
            .map_err(|_| missing("encoding_plan"))?;
        let observed = encoding.clone();
        let step = tokio::task::spawn_blocking(move || plan.advance(&observed))
            .await
            .map_err(|_| missing("worker"))?
            .map_err(|_| missing("encoding_step"))?;
        let shell = state.peek();
        if !facts.current(&shell) || !policy.current(&shell) {
            return Err(missing("owner"));
        }
        match step {
            EncodingStep::Complete(_) => {
                return Ok(Captured {
                    facts,
                    _custom: custom,
                    policy,
                    prepared,
                    headers,
                    encoding,
                    unfurled: None,
                });
            }
            EncodingStep::Need(EncodingNeed::File(id)) => {
                if encoding.files.contains_key(&id) {
                    return Err(missing("file"));
                }
                let file = FileObservation::capture(&shell, binding.owner_team(), &id)?;
                let raw: super_platinum_core::slack::models::File =
                    serde_json::from_value(file.source().clone()).map_err(|_| missing("file"))?;
                encoding
                    .files
                    .insert(id, Some(super_platinum_core::slack_lists::is_list(&raw)));
                facts.files.push(file);
            }
            EncodingStep::Need(need @ (EncodingNeed::Emoji(_) | EncodingNeed::UnicodeName(_))) => {
                drop(shell);
                if standard.is_none() {
                    standard = Some(emoji::acquire(state, &facts, policy, &proof).await?);
                }
                let catalog = standard
                    .as_ref()
                    .ok_or_else(|| missing("standard_catalog"))?;
                match need {
                    EncodingNeed::Emoji(text) => {
                        // N=false skips colon preparation, so a retained native
                        // custom embed can first reach its catalog here.
                        if custom::needs_catalog(catalog, custom.as_deref(), &text)? {
                            custom = Some(acquire_custom(state, &mut facts, policy).await?);
                        }
                        let value = custom::encoding(catalog, custom.as_deref(), &text)?;
                        encoding.emoji.insert(text, value);
                    }
                    EncodingNeed::UnicodeName(unicode) => {
                        encoding.unicode_names.insert(
                            unicode.clone(),
                            catalog.unicode_name(&unicode).map(str::to_owned),
                        );
                    }
                    EncodingNeed::File(_) => unreachable!(),
                }
            }
        }
    }
    Err(missing("observation_limit"))
}

// Shared by preparation and encoding reaches. Pin once before the real read;
// never replace the original revision or publish the result into another store.
async fn acquire_custom(
    state: Signal<ShellState>,
    facts: &mut Facts,
    policy: &own_edit_policy::Observation,
) -> Result<Arc<custom::Complete>, Error> {
    let shell = state.peek();
    if !facts.current(&shell) || !policy.current(&shell) {
        return Err(missing("owner"));
    }
    if facts.custom_revision.is_none() {
        facts.custom_revision = Some(
            shell
                .core
                .workspaces
                .get(&facts.context.session.team_id)
                .ok_or_else(|| missing("owner"))?
                .custom_emoji_revision,
        );
    }
    drop(shell);
    custom::acquire(state, facts, policy).await
}
