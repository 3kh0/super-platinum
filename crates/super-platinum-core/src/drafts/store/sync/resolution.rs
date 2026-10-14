//! Explicit local-version settlement. This never prepares or dispatches Slack
//! work, acknowledges a remote draft, or retires an immutable consume intent.
use super::*;
use crate::drafts::codec;
mod remote;
pub use remote::{RemoteChoice, RemoteReview, RemoteSettlement};

#[derive(Clone, Debug)]
pub struct LocalReview {
    pub scope: Scope,
    pub local: LocalDraft,
    pub stored: Option<Stored>,
    pub blocked: Option<BlockedLocal>,
    pub alternates: Vec<LocalAlternate>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocalChoice {
    Current,
    Blocked { token: String },
    Alternate { writer_id: String, revision: u64 },
}
#[derive(Debug)]
pub enum LocalSettlement {
    Applied {
        local: LocalDraft,
        stored: Option<Stored>,
    },
    /// Any member of the reviewed inventory, including the paired state,
    /// changed. Reload and let the human review all versions again.
    Stale,
    /// Pending legacy writes or unsupported managed content require another
    /// recovery step. No version was removed and no local content changed.
    Blocked,
}
fn same_local(a: &LocalDraft, b: &LocalDraft) -> bool {
    matches!((serde_json::to_value(a), serde_json::to_value(b)), (Ok(a), Ok(b)) if a == b)
}
impl LocalReview {
    pub fn contains_local(&self, local: &LocalDraft) -> bool {
        same_local(&self.local, local)
            || self
                .blocked
                .as_ref()
                .is_some_and(|row| same_local(&row.draft, local))
            || self
                .alternates
                .iter()
                .any(|row| same_local(&row.draft, local))
    }
    pub fn chosen_local(&self, choice: &LocalChoice) -> Option<&LocalDraft> {
        match choice {
            LocalChoice::Current => Some(&self.local),
            LocalChoice::Blocked { token } => self
                .blocked
                .as_ref()
                .filter(|row| &row.token == token)
                .map(|row| &row.draft),
            LocalChoice::Alternate {
                writer_id,
                revision,
            } => self
                .alternates
                .iter()
                .find(|row| &row.writer_id == writer_id && &row.revision == revision)
                .map(|row| &row.draft),
        }
    }
    fn matches(&self, other: &Self) -> bool {
        self.scope == other.scope
            && same_local(&self.local, &other.local)
            && match (&self.stored, &other.stored) {
                (None, None) => true,
                (Some(a), Some(b)) => a.revision == b.revision && a.state == b.state,
                _ => false,
            }
            && match (&self.blocked, &other.blocked) {
                (None, None) => true,
                (Some(a), Some(b)) => a.token == b.token && same_local(&a.draft, &b.draft),
                _ => false,
            }
            && self.alternates.len() == other.alternates.len()
            && self.alternates.iter().zip(&other.alternates).all(|(a, b)| {
                a.writer_id == b.writer_id
                    && a.revision == b.revision
                    && same_local(&a.draft, &b.draft)
            })
    }
}
fn review(conn: &Connection, scope: &Scope) -> Result<LocalReview, AppError> {
    if scope.key.team.is_empty()
        || scope.key.channel.is_empty()
        || scope.key.thread.as_ref().is_some_and(String::is_empty)
    {
        return Err(invalid("invalid local review scope"));
    }
    let key = serde_json::to_string(&scope.key)?;
    let content: Option<String> = conn
        .query_row(
            "SELECT content FROM composer_drafts WHERE destination=?1",
            [&key],
            |row| row.get(0),
        )
        .optional()?;
    let raw_blocked: Option<(String, String)> = conn
        .query_row(
            "SELECT token,content FROM composer_draft_blocked WHERE destination=?1",
            [&key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let blocked = raw_blocked
        .map(|(token, content)| -> Result<_, AppError> {
            Ok(BlockedLocal {
                token,
                draft: serde_json::from_str(&content)?,
            })
        })
        .transpose()?;
    Ok(LocalReview {
        scope: scope.clone(),
        local: content
            .map(|raw| serde_json::from_str(&raw))
            .transpose()?
            .unwrap_or_default(),
        stored: read(conn, scope)?,
        blocked,
        alternates: read_alternates(conn)?
            .remove(&scope.key)
            .unwrap_or_default(),
    })
}
impl DraftStore {
    /// Obtain one transactional review inventory, without normalizing live work.
    pub fn read_local_review(&mut self, scope: &Scope) -> Result<LocalReview, AppError> {
        let tx = self
            .0
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        owner(&tx, &scope.account_id)?;
        let result = review(&tx, scope)?;
        tx.commit()?;
        Ok(result)
    }

    pub fn settle_local_versions(
        &mut self,
        expected: &LocalReview,
        choice: &LocalChoice,
    ) -> Result<LocalSettlement, AppError> {
        let tx = self
            .0
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let scope = &expected.scope;
        owner(&tx, &scope.account_id)?;
        let current = review(&tx, scope)?;
        if !expected.matches(&current) {
            return Ok(LocalSettlement::Stale);
        }
        let Some(local) = current.chosen_local(choice).cloned() else {
            return Ok(LocalSettlement::Stale);
        };
        let mut stored = current.stored.clone();
        if let Some(row) = &mut stored {
            if !same_local(&current.local, &local) {
                let desired = if local.composer.text.is_empty() && local.attachments.is_empty() {
                    Ok(None)
                } else if local.attachments.is_empty() {
                    codec::encode_composer(&scope.key, &local.composer).map(Some)
                } else {
                    Err(codec::Unsupported::Files)
                };
                match desired {
                    Ok(desired) => {
                        row.state.edit(scope, desired);
                    }
                    Err(_) => {
                        // The only established opaque-local capability belongs
                        // to an immutable direct-send intent, never remote autosave.
                        let Some(intent) = row.state.consumption().cloned() else {
                            return Ok(LocalSettlement::Blocked);
                        };
                        if row.state.checkpoint_direct_local(&intent, None).is_err() {
                            return Ok(LocalSettlement::Blocked);
                        }
                    }
                }
            }
            row.revision = next_revision(current.stored.as_ref())?;
            validate(scope, &row.state)?;
            write(&tx, row)?;
        }
        write_local(&tx, &scope.key, &local)?;
        let key = serde_json::to_string(&scope.key)?;
        for alternate in &current.alternates {
            tx.execute("INSERT INTO composer_draft_reviewed_writers(destination,writer_id,revision) VALUES(?1,?2,?3) ON CONFLICT(destination,writer_id) DO UPDATE SET revision=MAX(revision,excluded.revision)", params![&key,&alternate.writer_id,alternate.revision as i64])?;
        }
        tx.execute(
            "DELETE FROM composer_draft_blocked WHERE destination=?1",
            [&key],
        )?;
        tx.execute(
            "DELETE FROM composer_draft_alternates WHERE destination=?1",
            [&key],
        )?;
        tx.commit()?;
        Ok(LocalSettlement::Applied { local, stored })
    }
}

#[cfg(test)]
mod tests;
