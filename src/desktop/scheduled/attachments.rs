//! Retained, app-root-owned attachment editing. This registry must not live in a
//! dialog or ScheduledState (which is replaced on account switches).
//!
//! Integration reserves the same registry for existing text editors and every
//! scheduled mutation. Async adapters retain Lease/Attempt, check captured Context
//! after awaits, pass raw envelopes, and dispatch only Attempt::request(). No
//! transport, session persistence or global file permissions are changed here.
use crate::state::ShellState;
use serde_json::Value;
use std::time::{Duration, Instant};
use std::{collections::BTreeMap, sync::Arc};
use super_platinum_core::{
    scheduled::{
        ScheduledMessage,
        edit::{AttachmentEdit, EditError, EditPhase, FrozenAttachmentEdit},
    },
    slack::{api::ScheduleMessage, models::File},
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Owner {
    account: String,
    team: String,
    user: String,
    draft: String,
}

impl Owner {
    pub fn capture(shell: &ShellState, draft: &str) -> Option<Self> {
        if crate::fixture::is_fixture() {
            return fixture::owner(shell, draft);
        }
        let account = shell.core.active_account.clone()?;
        let team = shell.core.active_team.clone()?;
        let session = shell.core.session.as_ref()?.workspaces.get(&team)?;
        if account.is_empty() || team.is_empty() || session.user_id.is_empty() || draft.is_empty() {
            return None;
        }
        Some(Self {
            account,
            team,
            user: session.user_id.clone(),
            draft: draft.into(),
        })
    }
}

/// Clones refer to the same retained visit; a stale lease cannot affect a reopened editor.
#[derive(Debug, Clone)]
pub(crate) struct Lease {
    owner: Owner,
    identity: Arc<()>,
}

impl PartialEq for Lease {
    fn eq(&self, other: &Self) -> bool {
        self.owner == other.owner && Arc::ptr_eq(&self.identity, &other.identity)
    }
}

impl Lease {
    pub fn draft_id(&self) -> &str {
        &self.owner.draft
    }
    pub fn matches_shell(&self, shell: &ShellState) -> bool {
        Owner::capture(shell, &self.owner.draft).as_ref() == Some(&self.owner)
    }

    pub fn current(&self, shell: &ShellState, context: &super::Context) -> bool {
        context.current(shell)
            && context.session.user_id == self.owner.user
            && self.matches_shell(shell)
            && shell
                .core
                .session
                .as_ref()
                .and_then(|session| session.workspaces.get(&self.owner.team))
                .is_some_and(|current| same_credentials(&context.session, current))
    }
}

fn same_credentials(
    captured: &super_platinum_core::config::WorkspaceSession,
    current: &super_platinum_core::config::WorkspaceSession,
) -> bool {
    captured.team_id == current.team_id
        && captured.user_id == current.user_id
        && captured.token == current.token
        && captured.url == current.url
        && captured.enterprise_id == current.enterprise_id
}

#[derive(Debug, Clone)]
pub(crate) struct Attempt {
    lease: Lease,
    frozen: FrozenAttachmentEdit,
}

impl Attempt {
    pub fn lease(&self) -> &Lease {
        &self.lease
    }
    pub fn draft_id(&self) -> &str {
        self.frozen.draft_id()
    }
    pub fn request(&self) -> ScheduleMessage {
        self.frozen.request()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fault {
    Busy,
    StaleLease,
    WrongDraft,
    QueuedRevisionChanged,
    Retained,
    CoolingDown,
    Edit(EditError),
}

impl From<EditError> for Fault {
    fn from(error: EditError) -> Self {
        Self::Edit(error)
    }
}

#[derive(Debug)]
struct Editing {
    original: ScheduledMessage,
    model: AttachmentEdit,
    attempt: Option<Attempt>,
    files: BTreeMap<String, File>,
    error: Option<String>,
    reviewing: bool,
    review_after: Option<(Instant, Duration)>,
}

#[derive(Debug)]
enum Reservation {
    Attachments(Editing),
    /// Existing text editor or scheduled mutation; never an attachment operation.
    Other(other::Other),
}

#[derive(Debug)]
struct Entry {
    lease: Lease,
    reservation: Reservation,
}

#[derive(Debug, Default)]
pub(crate) struct Operations {
    entries: BTreeMap<Owner, Entry>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FileRow {
    pub id: String,
    pub title: String,
    pub detail: String,
    pub removed: bool,
    pub preview_available: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Snapshot {
    pub phase: EditPhase,
    pub blocks: Vec<Value>,
    pub destinations: Vec<Value>,
    pub post_at: i64,
    pub rows: Vec<FileRow>,
    pub changed: bool,
    pub error: Option<String>,
    pub reviewing: bool,
    pub review_cooling_down: bool,
}

impl Operations {
    pub fn blocked(&self, owner: &Owner) -> bool {
        self.entries.contains_key(owner)
    }

    /// Retained operations remain discoverable even if a refreshed queue omits
    /// their draft. Absence must not hide the only path to read-only review.
    pub fn retained(&self, shell: &ShellState) -> Vec<Lease> {
        self.entries
            .values()
            .filter_map(|entry| {
                matches!(entry.reservation, Reservation::Attachments(_))
                    .then_some(&entry.lease)
                    .filter(|lease| lease.matches_shell(shell))
                    .cloned()
            })
            .collect()
    }

    /// Opening an existing attachment operation resumes its exact staged/uncertain
    /// state. It never reconstructs a fresh model from a subsequently loaded row.
    pub fn open(&mut self, owner: Owner, draft: ScheduledMessage) -> Result<Lease, Fault> {
        if owner.draft != draft.id {
            return Err(Fault::WrongDraft);
        }
        if let Some(entry) = self.entries.get(&owner) {
            return match entry.reservation {
                Reservation::Attachments(_) => Ok(entry.lease.clone()),
                Reservation::Other(_) => Err(Fault::Busy),
            };
        }
        let model = AttachmentEdit::new(draft.clone())?;
        let lease = Lease {
            owner: owner.clone(),
            identity: Arc::new(()),
        };
        self.entries.insert(
            owner,
            Entry {
                lease: lease.clone(),
                reservation: Reservation::Attachments(Editing {
                    original: draft,
                    model,
                    attempt: None,
                    files: BTreeMap::new(),
                    error: None,
                    reviewing: false,
                    review_after: None,
                }),
            },
        );
        Ok(lease)
    }

    pub fn reserve_other(&mut self, owner: Owner) -> Result<Lease, Fault> {
        if self.blocked(&owner) {
            return Err(Fault::Busy);
        }
        let lease = Lease {
            owner: owner.clone(),
            identity: Arc::new(()),
        };
        self.entries.insert(
            owner,
            Entry {
                lease: lease.clone(),
                reservation: Reservation::Other(other::Other::default()),
            },
        );
        Ok(lease)
    }

    pub fn release_other(&mut self, lease: &Lease) -> Result<(), Fault> {
        self.validate_other(lease)?;
        if !self.other(lease)?.idle() {
            return Err(Fault::Retained);
        }
        self.entries.remove(&lease.owner);
        Ok(())
    }

    /// Existing editor saves reuse their retained reservation. Revalidate after
    /// each await so closing/reopening cannot authorize an old async callback.
    pub fn validate_other(&self, lease: &Lease) -> Result<(), Fault> {
        if matches!(&self.entry(lease)?.reservation, Reservation::Other(other) if !other.uncertain())
        {
            Ok(())
        } else {
            Err(Fault::Retained)
        }
    }

    /// Close is a view action and need not call this. Cancel discards only staging
    /// before dispatch; in-flight/uncertain entries remain reserved across navigation.
    pub fn cancel(&mut self, lease: &Lease) -> Result<(), Fault> {
        let editing = self.editing(lease)?;
        if matches!(
            editing.model.phase(),
            EditPhase::InFlight | EditPhase::Uncertain
        ) {
            return Err(Fault::Retained);
        }
        self.entries.remove(&lease.owner);
        Ok(())
    }

    pub fn remove(&mut self, lease: &Lease, id: &str) -> Result<(), Fault> {
        let editing = self.editing_mut(lease)?;
        editing.model.remove(id)?;
        editing.error = None;
        Ok(())
    }

    pub fn restore(&mut self, lease: &Lease, id: &str) -> Result<(), Fault> {
        let editing = self.editing_mut(lease)?;
        editing.model.restore(id)?;
        editing.error = None;
        Ok(())
    }

    pub fn freeze(&mut self, lease: &Lease) -> Result<Attempt, Fault> {
        let editing = self.editing_mut(lease)?;
        if cooling_down(editing) {
            return Err(Fault::CoolingDown);
        }
        let frozen = match editing.model.freeze() {
            Ok(frozen) => frozen,
            Err(error) => {
                editing.error = Some(match error {
                    EditError::EmptyOrUnprovenContent => "Keep an attachment or add a message in the message editor before removing the last file.",
                    EditError::Unchanged => "Remove an attachment before saving.",
                    _ => "This attachment edit cannot be saved in its current state.",
                }.into());
                return Err(error.into());
            }
        };
        let attempt = Attempt {
            lease: lease.clone(),
            frozen,
        };
        editing.attempt = Some(attempt.clone());
        editing.error = None;
        Ok(attempt)
    }

    pub fn preflight_failed(&mut self, attempt: &Attempt, message: String) -> Result<(), Fault> {
        let editing = self.editing_mut(&attempt.lease)?;
        editing.model.abandon_preflight(&attempt.frozen)?;
        editing.attempt = None;
        editing.error = Some(message);
        Ok(())
    }

    /// Adapter must synchronously check lease.current(captured context) immediately
    /// before this call and mutation dispatch; no await separates these operations.
    pub fn admit(&mut self, attempt: &Attempt, response: &Value) -> Result<(), Fault> {
        self.editing_mut(&attempt.lease)?
            .model
            .admit_dispatch(&attempt.frozen, response)?;
        Ok(())
    }

    /// The mounted queue may know a revision newer than a lagging info read.
    /// Compare every represented field, including at equal or absent stamps.
    /// Absence is not evidence of deletion; the positive info preflight still
    /// supplies complete admission when no queue row is present.
    pub fn admit_current(
        &mut self,
        attempt: &Attempt,
        response: &Value,
        queue: &super_platinum_core::scheduled::ScheduledState,
    ) -> Result<(), Fault> {
        let editing = self.editing_mut(&attempt.lease)?;
        if !queue.review_is_current(&editing.original) {
            editing.model.reject_stale_preflight(&attempt.frozen)?;
            return Err(Fault::QueuedRevisionChanged);
        }
        editing.model.admit_dispatch(&attempt.frozen, response)?;
        Ok(())
    }

    pub fn acknowledge(&mut self, attempt: &Attempt, response: &Value) -> Result<bool, Fault> {
        Ok(self
            .editing_mut(&attempt.lease)?
            .model
            .acknowledge(&attempt.frozen, response)?)
    }

    /// Always record a captured mutation's uncertain outcome even after navigation.
    /// Current-shell admission gates visual updates, not retention of this outcome.
    pub fn uncertain(&mut self, attempt: &Attempt) -> Result<(), Fault> {
        self.editing_mut(&attempt.lease)?
            .model
            .mark_uncertain(&attempt.frozen)?;
        Ok(())
    }

    pub fn not_dispatched(&mut self, attempt: &Attempt) -> Result<(), Fault> {
        let editing = self.editing_mut(&attempt.lease)?;
        editing.model.not_dispatched(&attempt.frozen)?;
        editing.attempt = None;
        editing.error =
            Some("Reconnect to Slack before saving. No attachment update was sent.".into());
        Ok(())
    }

    pub fn rejected(&mut self, attempt: &Attempt, response: &Value) -> Result<bool, Fault> {
        let editing = self.editing_mut(&attempt.lease)?;
        if !editing.model.rejected(&attempt.frozen, response)? {
            return Ok(false);
        }
        editing.attempt = None;
        let code = super_platinum_core::scheduled::edit::definite_rejection(response)
            .expect("model accepted only a recognized rejection");
        editing.error = Some(super::failure(&super_platinum_core::slack::Error::Api(
            code.into(),
        )));
        Ok(true)
    }

    pub fn review_attempt(&mut self, lease: &Lease) -> Result<Attempt, Fault> {
        let editing = self.editing_mut(lease)?;
        if editing.model.phase() != EditPhase::Uncertain {
            return Err(Fault::Retained);
        }
        if editing.reviewing {
            return Err(Fault::Busy);
        }
        if cooling_down(editing) {
            return Err(Fault::CoolingDown);
        }
        let attempt = editing.attempt.clone().ok_or(Fault::StaleLease)?;
        editing.reviewing = true;
        Ok(attempt)
    }

    pub fn review(&mut self, attempt: &Attempt, response: Option<&Value>) -> Result<bool, Fault> {
        let editing = self.editing_mut(&attempt.lease)?;
        if !editing.reviewing {
            return Err(Fault::Retained);
        }
        let confirmed = editing.model.review(&attempt.frozen, response)?;
        editing.reviewing = false;
        Ok(confirmed)
    }

    pub fn set_error(&mut self, lease: &Lease, message: String) -> Result<(), Fault> {
        self.editing_mut(lease)?.error = Some(message);
        Ok(())
    }

    pub fn review_cooldown(&mut self, lease: &Lease, seconds: u64) -> Result<(), Fault> {
        self.editing_mut(lease)?.review_after =
            Some((Instant::now(), Duration::from_secs(seconds)));
        Ok(())
    }

    pub fn refresh_cooldown(&mut self, lease: &Lease) -> Result<(), Fault> {
        let editing = self.editing_mut(lease)?;
        if !cooling_down(editing) {
            editing.review_after = None;
        }
        Ok(())
    }

    /// Metadata completion is lease scoped and exact-ID checked. A missing file
    /// stays removable under its original ID, with no fabricated name or preview.
    pub fn metadata(&mut self, lease: &Lease, requested: &str, file: File) -> Result<(), Fault> {
        let editing = self.editing_mut(lease)?;
        if file.id.as_deref() != Some(requested)
            || !editing.original.file_ids.iter().any(|id| id == requested)
        {
            return Err(Fault::WrongDraft);
        }
        editing.files.insert(requested.into(), file);
        Ok(())
    }

    /// Caller uses the existing files::open after checking current captured context.
    pub fn preview(&self, lease: &Lease, id: &str) -> Result<Option<File>, Fault> {
        Ok(self.editing(lease)?.files.get(id).cloned())
    }

    pub fn snapshot(&self, lease: &Lease) -> Result<Snapshot, Fault> {
        let editing = self.editing(lease)?;
        Ok(Snapshot {
            phase: editing.model.phase(),
            blocks: editing.original.blocks.clone(),
            destinations: editing.original.destinations.clone(),
            post_at: editing.original.post_at,
            changed: editing.model.file_ids() != editing.original.file_ids,
            error: editing.error.clone(),
            reviewing: editing.reviewing,
            review_cooling_down: cooling_down(editing),
            rows: editing
                .original
                .file_ids
                .iter()
                .map(|id| {
                    let file = editing.files.get(id);
                    FileRow {
                        id: id.clone(),
                        title: file
                            .map(super_platinum_core::state::file_title)
                            .unwrap_or_else(|| id.clone()),
                        detail: file
                            .map(|file| {
                                [
                                    file.pretty_type.clone().or_else(|| file.filetype.clone()),
                                    file.size.map(super_platinum_core::state::format_file_size),
                                ]
                                .into_iter()
                                .flatten()
                                .collect::<Vec<_>>()
                                .join(" · ")
                            })
                            .unwrap_or_else(|| "File details unavailable".into()),
                        removed: !editing.model.file_ids().contains(id),
                        preview_available: file.is_some(),
                    }
                })
                .collect(),
        })
    }

    fn entry(&self, lease: &Lease) -> Result<&Entry, Fault> {
        self.entries
            .get(&lease.owner)
            .filter(|entry| Arc::ptr_eq(&entry.lease.identity, &lease.identity))
            .ok_or(Fault::StaleLease)
    }
    fn editing(&self, lease: &Lease) -> Result<&Editing, Fault> {
        match &self.entry(lease)?.reservation {
            Reservation::Attachments(editing) => Ok(editing),
            Reservation::Other(_) => Err(Fault::Busy),
        }
    }
    fn editing_mut(&mut self, lease: &Lease) -> Result<&mut Editing, Fault> {
        self.entry(lease)?;
        match &mut self
            .entries
            .get_mut(&lease.owner)
            .ok_or(Fault::StaleLease)?
            .reservation
        {
            Reservation::Attachments(editing) => Ok(editing),
            Reservation::Other(_) => Err(Fault::Busy),
        }
    }
}

fn cooling_down(editing: &Editing) -> bool {
    editing
        .review_after
        .is_some_and(|(started, duration)| started.elapsed() < duration)
}

pub(crate) mod cleanup;
pub(crate) mod fixture;
pub(crate) mod native;
pub(crate) mod other;
#[cfg(test)]
mod tests;
