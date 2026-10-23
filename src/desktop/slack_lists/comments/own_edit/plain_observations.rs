//! Reached plain EDIT inputs from current canonical stores and fresh policy.
//! This captures content, not permission to edit or send. Unavailable metadata
//! never becomes an empty result, and Profile query authority is not borrowed.
use super::*;
use crate::{app_home::channel_fetches::canonical, directory_member_fetches::Context};
use std::collections::BTreeSet;
use super_platinum_core::{
    composer_emoji::standard::Catalog,
    slack::models::UserGroup,
    slack_lists::comments::{
        edit_seed::{
            SeedUnavailable,
            plain::{PlainObservations, PlainSeed},
        },
        ordered::MessageWitness,
    },
    state::{directory_members::Subset, directory_usergroups::Snapshot},
};
mod emoji;
mod preferences;
pub(super) use preferences::{emoji_version, highlights, native_policy};
mod recovery;
#[cfg(test)]
mod tests;

const GENERATION: &str = "list-own-edit-plain-initialization";

pub(super) struct Captured {
    facts: Facts,
    pub(super) seed: PlainSeed,
    pub(super) native_policy: preferences::NativePolicy,
}
impl Captured {
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        self.facts.current(shell)
    }
}
struct Facts {
    source: Source,
    serial: u64,
    context: Context,
    preferences_revision: u64,
    members: Vec<Subset>,
    channels: Option<Arc<canonical::Canonical>>,
    groups: Option<Snapshot>,
    emoji: Option<Arc<EmojiFacts>>,
}
impl Facts {
    /// Recheck after every acquisition/await and immediately before installing
    /// content. The caller must also retain and check its fresh policy receipt.
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        self.source.current(shell)
            && self.source.owner.reading(shell, self.serial)
            && self.context.current(shell)
            && self.preferences_revision
                == crate::app_home::autocomplete::preferences_revision(shell)
            && self.channels.as_ref().is_none_or(|v| v.current(shell))
            && shell
                .core
                .workspaces
                .get(&self.context.session.team_id)
                .is_some_and(|w| {
                    self.emoji
                        .as_ref()
                        .is_none_or(|e| w.locale.as_deref() == Some(e.locale.as_str()))
                        && self.members.iter().all(|subset| {
                            subset.current_rows(
                                &w.directory_members,
                                &self.context.account,
                                &self.context.session,
                                self.context.epoch,
                                GENERATION,
                                &w.users,
                            )
                        })
                        && self.groups.as_ref().is_none_or(|groups| {
                            groups.current(
                                &w.directory_usergroups,
                                &self.context.account,
                                &self.context.session,
                                self.context.epoch,
                                &w.usergroups,
                            )
                        })
                })
    }
}

/// Admission of current facts. Reached unavailable members use the existing
/// shared acquisition manager, retaining this original opening across its work.
/// The core converter asks only for reached observations, including the exact
/// original emoji element. Ignored/preserved blocks cause no metadata reads.
pub(super) async fn capture(
    state: Signal<ShellState>,
    source: &Source,
    serial: u64,
    policy: &own_edit_policy::Observation,
    original: &MessageWitness,
) -> Result<Captured, Error> {
    let (proof, context, preferences_revision) = {
        let shell = state.peek();
        if !source.current(&shell)
            || !source.owner.reading(&shell, serial)
            || !policy.current(&shell)
        {
            return Err(unavailable());
        }
        let proof = policy.proof();
        if proof["plainTextMode"] != true {
            return Err(unavailable());
        }
        let message: Message =
            serde_json::from_value(original.raw_message().clone()).map_err(|_| unavailable())?;
        if !source.same(&message) || message.ts.as_deref() != Some(&source.ts) {
            return Err(unavailable());
        }
        (
            proof,
            Context::capture(&shell, &source.owner.scope.team)?,
            crate::app_home::autocomplete::preferences_revision(&shell),
        )
    };
    let mut facts = Facts {
        source: source.clone(),
        serial,
        context,
        preferences_revision,
        members: Vec::new(),
        channels: None,
        groups: None,
        emoji: None,
    };
    let native_policy =
        preferences::native_policy(&proof["userPreferences"], &proof["assignments"])?;
    if !native_policy.plain_text_mode {
        return Err(unavailable());
    }
    let original = Arc::new(original.clone());
    let mut observations = PlainObservations::default();
    // A finite bound also prevents a malformed converter demand from looping.
    // Every successful iteration must add a previously absent exact fact.
    for _ in 0..256 {
        if !facts.current(&state.peek()) || !policy.current(&state.peek()) {
            return Err(unavailable());
        }
        // Conversion is CPU work over bounded owned values. A long comment
        // with many references must not repeat that work on the renderer task.
        let original = original.clone();
        let observed = observations.clone();
        let result = tokio::task::spawn_blocking(move || PlainSeed::capture(&original, &observed))
            .await
            .map_err(|_| unavailable())?;
        let shell = state.peek();
        if !facts.current(&shell) || !policy.current(&shell) {
            return Err(unavailable());
        }
        let context = &facts.context;
        let w = shell
            .core
            .workspaces
            .get(&context.session.team_id)
            .ok_or_else(unavailable)?;
        match result {
            Ok(seed) => {
                return Ok(Captured {
                    facts,
                    seed,
                    native_policy,
                });
            }
            Err(SeedUnavailable::UnknownEmojiPreference) => {
                if observations.seed.emoji_as_text.is_some() {
                    return Err(unavailable());
                }
                observations.seed.emoji_as_text =
                    Some(preferences::emoji_as_text(&proof["userPreferences"])?);
            }
            Err(SeedUnavailable::MissingHighlights) => {
                if observations.highlight_words.is_some() {
                    return Err(unavailable());
                }
                let snapshot = group_snapshot(&mut facts.groups, w, context)?;
                observations.highlight_words = Some(preferences::highlights(
                    &proof["userPreferences"],
                    &context.session.user_id,
                    snapshot.membership().ok_or_else(unavailable)?,
                )?);
            }
            Err(SeedUnavailable::MissingUser(id)) => {
                if observations.seed.users.contains_key(&id) {
                    return Err(unavailable());
                }
                let existing = recovery::member(&shell, context, &id);
                drop(shell);
                let (subset, label) = match existing {
                    Ok(value) => value,
                    Err(_) => {
                        recovery::member_after_acquisition(state, &facts, policy, &id).await?
                    }
                };
                observations.seed.users.insert(id, label);
                facts.members.push(subset);
            }
            Err(SeedUnavailable::MissingChannel(id)) => {
                if observations.seed.channels.contains_key(&id) {
                    return Err(unavailable());
                }
                if facts.channels.is_none() {
                    facts.channels = Some(canonical::observed(&shell, &context.session.team_id)?);
                }
                let row = facts
                    .channels
                    .as_ref()
                    .and_then(|c| c.row(&id))
                    .ok_or_else(unavailable)?;
                let label =
                    canonical::ordinary_label(&json!({"type":"channel","channel_id":id}), row)?;
                observations.seed.channels.insert(id, label);
            }
            Err(SeedUnavailable::MissingChannelToken(raw)) => {
                let id = raw["channel_id"].as_str().ok_or_else(unavailable)?;
                if observations
                    .channel_labels
                    .iter()
                    .any(|(before, _)| before == &raw)
                {
                    return Err(unavailable());
                }
                if facts.channels.is_none() {
                    facts.channels = Some(canonical::observed(&shell, &context.session.team_id)?);
                }
                let row = facts
                    .channels
                    .as_ref()
                    .and_then(|c| c.row(id))
                    .ok_or_else(unavailable)?;
                let label = canonical::ordinary_label(&raw, row)?;
                observations.channel_labels.push((raw, label));
            }
            Err(SeedUnavailable::MissingUsergroup(id)) => {
                if observations.seed.usergroups.contains_key(&id) {
                    return Err(unavailable());
                }
                let snapshot = group_snapshot(&mut facts.groups, w, context)?;
                let row = snapshot
                    .rows()
                    .iter()
                    .find(|row| row.effective["id"] == id)
                    .ok_or_else(unavailable)?;
                let group: UserGroup =
                    serde_json::from_value(row.effective.clone()).map_err(|_| unavailable())?;
                observations
                    .seed
                    .usergroups
                    .insert(id, group.profile_label().ok_or_else(unavailable)?);
            }
            Err(SeedUnavailable::MissingBroadcast(range)) => {
                if observations.seed.broadcasts.contains_key(&range) {
                    return Err(unavailable());
                }
                observations
                    .seed
                    .broadcasts
                    .insert(range.clone(), preferences::english_broadcast(&range)?);
            }
            Err(SeedUnavailable::MissingEmojiText(raw)) => {
                if observations
                    .emoji_text
                    .iter()
                    .any(|(before, _)| before == &raw)
                {
                    return Err(unavailable());
                }
                let locale = w.locale.clone().ok_or_else(unavailable)?;
                if facts.emoji.is_none() {
                    let version = preferences::emoji_version(&proof["assignments"])?;
                    let original_locale = locale.clone();
                    drop(shell);
                    let names = tokio::task::spawn_blocking(move || {
                        let catalog = Catalog::build(version).map_err(|_| unavailable())?;
                        emoji::EnglishCatalog::capture(&catalog, &locale)
                    })
                    .await
                    .map_err(|_| unavailable())??;
                    let shell = state.peek();
                    if !facts.current(&shell)
                        || !policy.current(&shell)
                        || shell
                            .core
                            .workspaces
                            .get(&facts.context.session.team_id)
                            .and_then(|w| w.locale.as_deref())
                            != Some(original_locale.as_str())
                    {
                        return Err(unavailable());
                    }
                    facts.emoji = Some(Arc::new(EmojiFacts {
                        locale: original_locale,
                        names,
                    }));
                }
                let emoji = facts.emoji.as_ref().ok_or_else(unavailable)?;
                let text = emoji.names.text(&raw)?;
                observations.emoji_text.push((raw, text));
            }
            // A complete semantic slug, not just a displayed file title, is
            // required. Preserve the unavailable result until that producer exists.
            Err(_) => return Err(unavailable()),
        }
    }
    Err(unavailable())
}

struct EmojiFacts {
    locale: String,
    names: emoji::EnglishCatalog,
}

pub(super) fn group_snapshot<'a>(
    slot: &'a mut Option<Snapshot>,
    workspace: &super_platinum_core::state::Workspace,
    context: &Context,
) -> Result<&'a Snapshot, Error> {
    if slot.is_none() {
        *slot = Some(
            workspace
                .directory_usergroups
                .snapshot(
                    &context.account,
                    &context.session,
                    context.epoch,
                    &workspace.usergroups,
                )
                .map_err(|_| unavailable())?,
        );
    }
    slot.as_ref().ok_or_else(unavailable)
}
