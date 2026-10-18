//! Closed disconnected baseline data, bound to one fresh temporary owned worker.
use super::*;
use crate::drafts::{DraftKey, sync::RemoteDraft};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BaselineFixtureTarget {
    Channel,
    Thread,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BaselineFixtureOutcome {
    Unchanged,
    Changed,
    /// An actual nonpositive read, reserve-time Cooldown, then explicit retry.
    /// This is not the distinct claim-time Deferred race.
    RateLimitedThenChanged,
    /// Existing Local choice rejects attachments; Remote remains explicit.
    ChangedWithAttachment,
}
#[derive(Clone)]
pub struct BaselineFixtureRead {
    pub scope: Scope,
    pub draft_id: String,
    /// None retains an actual request refused by the fixed fixture scope.
    pub read: Option<BaselineRead>,
}
struct RetainedRead {
    request: p::BaselineRequest,
    read: Option<BaselineRead>,
}
pub struct BaselineFixture {
    queue: Arc<Queue>,
    scope: Scope,
    remote: RemoteDraft,
    changed: crate::drafts::sync::Snapshot,
    outcome: BaselineFixtureOutcome,
    reads: Mutex<Vec<RetainedRead>>,
}
impl BaselineFixture {
    pub(in crate::drafts::store) fn create(
        queue: Arc<Queue>,
        target: BaselineFixtureTarget,
        outcome: BaselineFixtureOutcome,
    ) -> Result<Self, p::Error> {
        let scope = Scope {
            account_id: queue.account().into(),
            key: DraftKey {
                team: "T1".into(),
                channel: "C1".into(),
                thread: match target {
                    BaselineFixtureTarget::Channel => None,
                    BaselineFixtureTarget::Thread => Some("10.000001".into()),
                },
            },
        };
        let remote = RemoteDraft {
            scope: scope.clone(),
            id: "D_BASELINE_FIXTURE".into(),
            client_msg_id: "00000000-0000-4000-8000-000000000001".into(),
            last_updated_ts: "10.000001".into(),
            snapshot: crate::drafts::codec::encode(&scope.key, "Acknowledged *baseline* 日本語")
                .map_err(|_| p::Error::Unsupported)?,
        };
        let changed =
            crate::drafts::codec::encode(&scope.key, "Remote *changed* 日本語 🚀\nsecond line")
                .map_err(|_| p::Error::Unsupported)?;
        let mut local = LocalDraft {
            composer: crate::ComposerState::with_text(
                "Local *retained* 日本語 🚀 e\u{301}\nsecond line",
            ),
            ..Default::default()
        };
        local.composer.set_selection(6, 14);
        if outcome == BaselineFixtureOutcome::ChangedWithAttachment {
            local.attachments.push(crate::domain::ComposerAttachment {
                id: 17,
                path: "baseline-fixture-local.txt".into(),
                name: "retained 日本語.txt".into(),
                bytes: 7,
                uploading: false,
                upload_started: None,
                upload_cancel: None,
                upload_progress: None,
                preview_path: None,
            });
        }
        let mut state = crate::drafts::sync::DraftSync::adopt_remote(&scope, remote.clone())
            .map_err(|_| p::Error::Unsupported)?;
        let desired = crate::drafts::codec::encode_composer(&scope.key, &local.composer)
            .map_err(|_| p::Error::Unsupported)?;
        if !state.edit(&scope, Some(desired)) {
            return Err(p::Error::Unsupported);
        }
        // The private queue has not escaped. Seed ordinary durable data before
        // inspect/migrate/open, without fake reviews or a prepared network write.
        let mut store = DraftStore::open(queue.owner.path()).map_err(|_| p::Error::Storage)?;
        store
            .save([(scope.key.clone(), local.clone())])
            .map_err(|_| p::Error::Storage)?;
        if !matches!(
            store
                .commit_synced(&scope, None, &local, &state, None)
                .map_err(|_| p::Error::Storage)?,
            sync::Commit::Applied(_)
        ) {
            return Err(p::Error::Stale);
        }
        drop(store);
        Ok(Self {
            queue,
            scope,
            remote,
            changed,
            outcome,
            reads: Mutex::new(Vec::new()),
        })
    }
    pub fn scope(&self) -> &Scope {
        &self.scope
    }
    /// Read-only copies of bounded synthetic evidence, never response setters.
    pub fn reads(&self) -> Vec<BaselineFixtureRead> {
        self.reads
            .lock()
            .unwrap()
            .iter()
            .map(|v| BaselineFixtureRead {
                scope: v.request.scope().clone(),
                draft_id: v.request.draft_id().into(),
                read: v.read.clone(),
            })
            .collect()
    }
    /// No request argument or callback: take this exact completion's real request
    /// once, after same-worker validation. No mismatched/live transport fallback.
    /// Caller retains/binds the returned raw result through the existing facade.
    pub fn read(
        &self,
        completion: &BaselineCompletion,
        now_ms: u64,
    ) -> Result<BaselineRead, AdmissionError> {
        if !Arc::ptr_eq(&self.queue, &completion.queue) {
            return Err(AdmissionError::Stale);
        }
        let until = now_ms
            .checked_add(60_000)
            .filter(|v| *v <= i64::MAX as u64)
            .ok_or(AdmissionError::Counter)?;
        let mut reads = self.reads.lock().map_err(|_| AdmissionError::Dead)?;
        let limit = if self.outcome == BaselineFixtureOutcome::RateLimitedThenChanged {
            2
        } else {
            1
        };
        if reads.len() >= limit {
            return Err(AdmissionError::Stale);
        }
        let request = completion.take_request()?;
        if request.scope() != &self.scope || request.draft_id() != self.remote.id {
            reads.push(RetainedRead {
                request,
                read: None,
            });
            return Err(AdmissionError::Stale);
        }
        let read =
            if self.outcome == BaselineFixtureOutcome::RateLimitedThenChanged && reads.is_empty() {
                BaselineRead {
                    response: p::BaselineResponse::Nonpositive(p::KnownReadFailure::RateLimited),
                    known_until: until,
                }
            } else {
                let unchanged = self.outcome == BaselineFixtureOutcome::Unchanged;
                let snapshot = if unchanged {
                    &self.remote.snapshot
                } else {
                    &self.changed
                };
                BaselineRead {
                    response: p::BaselineResponse::Positive {
                        raw: serde_json::json!({
                            "ok":true,
                            "fixture_unknown_root":{"keep":["日本語",null,{"future":true}]},
                            "draft":{
                                "id":self.remote.id,"client_msg_id":self.remote.client_msg_id,
                                "last_updated_ts":if unchanged {"10.000001"} else {"11.000001"},
                                "team_id":"T1","user_id":"U1","is_deleted":false,
                                "is_sent":false,"date_scheduled":0,"attachments":[],
                                "blocks":snapshot.blocks,"file_ids":snapshot.file_ids,
                                "destinations":snapshot.destinations
                            }
                        }),
                        user: "U1".into(),
                    },
                    known_until: 0,
                }
            };
        reads.push(RetainedRead {
            request,
            read: Some(read.clone()),
        });
        Ok(read)
    }
}

#[cfg(test)]
#[path = "owned_baseline_fixture_tests.rs"]
mod tests;
