//! Exact preparatory storage calls, not a generic Action allowlist. No HTTP.
use super::*;
use p::commands::Action;

#[derive(Clone)]
pub struct GapInput {
    pub stream: p::Stream,
    pub reason: p::GapReason,
}
#[derive(Clone)]
pub struct TeamInput {
    pub scope: Scope,
}
#[derive(Clone)]
pub struct OpenInput {
    pub scope: Scope,
    pub expected: p::StreamState,
}
#[derive(Clone)]
pub(in crate::drafts::store::worker) struct TakeoverInput {
    pub scope: Scope,
    pub expected: Option<p::RecoveryState>,
}

impl Worker {
    pub(in crate::drafts::store::worker) fn hold_mark_gap(
        &self,
        input: GapInput,
    ) -> Result<Step<GapInput, ()>, Refused<GapInput>> {
        // Match Client::mark_gap: revocation is synchronous even when storage
        // admission refuses. Nothing here grants a new Stream or send permit.
        input.stream.revoke();
        let scope = Scope {
            account_id: input.stream.account().to_owned(),
            key: DraftKey {
                team: input.stream.team().to_owned(),
                channel: String::new(),
                thread: None,
            },
        };
        if !bounded(&scope.account_id, Some(&scope.key)) {
            return Err(refusal(
                &mut self.pending.lock().unwrap(),
                Reason::Metadata,
                input,
            ));
        }
        // Empty channel is metadata for this team-only action, not a fabricated
        // conversation. The executor consumes the actual opaque Stream only.
        self.step(&scope, input, "protected-mark-gap", |i, reply| {
            Action::MarkGap {
                stream: i.stream.clone(),
                reason: i.reason,
                reply,
            }
        })
    }

    pub(in crate::drafts::store::worker) fn hold_renew_local(
        &self,
        input: p::LocalInventory,
    ) -> Result<Step<p::LocalInventory, p::LocalWriterLease>, Refused<p::LocalInventory>> {
        if !bounded(&input.scope().account_id, Some(&input.scope().key)) {
            return Err(refusal(
                &mut self.pending.lock().unwrap(),
                Reason::Metadata,
                input,
            ));
        }
        let scope = input.scope().clone();
        self.step(&scope, input, "protected-renew-local", |i, reply| {
            Action::RenewLocal {
                expected: i.clone(),
                reply,
            }
        })
    }

    pub(in crate::drafts::store::worker) fn hold_local_inventory(
        &self,
        scope: Scope,
    ) -> Result<Step<Scope, p::LocalInventory>, Refused<Scope>> {
        if !bounded(&scope.account_id, Some(&scope.key)) {
            return Err(refusal(
                &mut self.pending.lock().unwrap(),
                Reason::Metadata,
                scope,
            ));
        }
        self.step(
            &scope.clone(),
            scope,
            "protected-local-inventory",
            |scope, reply| Action::ReviewLocal {
                scope: scope.clone(),
                reply,
            },
        )
    }
    pub(in crate::drafts::store::worker) fn hold_enroll(
        &self,
        input: p::LocalInventory,
    ) -> Result<Step<p::LocalInventory, p::Review>, Refused<p::LocalInventory>> {
        if !bounded(&input.scope().account_id, Some(&input.scope().key)) {
            return Err(refusal(
                &mut self.pending.lock().unwrap(),
                Reason::Metadata,
                input,
            ));
        }
        let scope = input.scope().clone();
        self.step(&scope, input, "protected-enroll-local", |i, reply| {
            Action::EnrollLocal {
                scope: i.scope().clone(),
                expected: i.clone(),
                reply,
            }
        })
    }
    pub(in crate::drafts::store::worker) fn hold_stream_state(
        &self,
        input: TeamInput,
    ) -> Result<Step<TeamInput, p::StreamState>, Refused<TeamInput>> {
        if !bounded(&input.scope.account_id, Some(&input.scope.key)) {
            return Err(refusal(
                &mut self.pending.lock().unwrap(),
                Reason::Metadata,
                input,
            ));
        }
        let scope = input.scope.clone();
        self.step(&scope, input, "protected-stream-state", |i, reply| {
            Action::StreamState {
                team: i.scope.key.team.clone(),
                reply,
            }
        })
    }
    pub(in crate::drafts::store::worker) fn hold_open_stream(
        &self,
        input: OpenInput,
    ) -> Result<Step<OpenInput, p::Stream>, Refused<OpenInput>> {
        if !bounded(&input.scope.account_id, Some(&input.scope.key)) {
            return Err(refusal(
                &mut self.pending.lock().unwrap(),
                Reason::Metadata,
                input,
            ));
        }
        let scope = input.scope.clone();
        self.step(&scope, input, "protected-open-stream", |i, reply| {
            Action::OpenStream {
                team: i.scope.key.team.clone(),
                expected: i.expected.clone(),
                reply,
            }
        })
    }
    pub(in crate::drafts::store::worker) fn hold_verify(
        &self,
        input: MutationInput,
    ) -> Result<Step<MutationInput, p::Review>, Refused<MutationInput>> {
        if !bounded(
            &input.expected.scope().account_id,
            Some(&input.expected.scope().key),
        ) {
            return Err(refusal(
                &mut self.pending.lock().unwrap(),
                Reason::Metadata,
                input,
            ));
        }
        let scope = input.expected.scope().clone();
        self.step(&scope, input, "protected-verify-local", |i, reply| {
            Action::VerifyLocal {
                stream: i.stream.clone(),
                expected: i.expected.clone(),
                reply,
            }
        })
    }
    pub(in crate::drafts::store::worker) fn hold_managed_review(
        &self,
        scope: Scope,
    ) -> Result<Step<Scope, p::Review>, Refused<Scope>> {
        if !bounded(&scope.account_id, Some(&scope.key)) {
            return Err(refusal(
                &mut self.pending.lock().unwrap(),
                Reason::Metadata,
                scope,
            ));
        }
        self.step(&scope.clone(), scope, "protected-review", |scope, reply| {
            Action::Review {
                scope: scope.clone(),
                reply,
            }
        })
    }
    pub(in crate::drafts::store::worker) fn hold_recovery_state(
        &self,
        input: TeamInput,
    ) -> Result<Step<TeamInput, Option<p::RecoveryState>>, Refused<TeamInput>> {
        if !bounded(&input.scope.account_id, Some(&input.scope.key)) {
            return Err(refusal(
                &mut self.pending.lock().unwrap(),
                Reason::Metadata,
                input,
            ));
        }
        let scope = input.scope.clone();
        self.step(&scope, input, "protected-recovery-state", |i, reply| {
            Action::RecoveryState {
                team: i.scope.key.team.clone(),
                reply,
            }
        })
    }
    pub(in crate::drafts::store::worker) fn hold_takeover(
        &self,
        input: TakeoverInput,
    ) -> Result<Step<TakeoverInput, p::Recovery>, Refused<TakeoverInput>> {
        if !bounded(&input.scope.account_id, Some(&input.scope.key)) {
            return Err(refusal(
                &mut self.pending.lock().unwrap(),
                Reason::Metadata,
                input,
            ));
        }
        let scope = input.scope.clone();
        self.step(&scope, input, "protected-takeover", |i, reply| {
            Action::TakeOver {
                team: i.scope.key.team.clone(),
                expected: i.expected.clone(),
                reply,
            }
        })
    }
    // Retrieves only the existing pending operation's actual receipt. This is
    // storage recovery input, not a ClaimedDispatch or permission to send.
    pub(in crate::drafts::store::worker) fn hold_pending_receipt(
        &self,
        input: p::Review,
    ) -> Result<Step<p::Review, p::Receipt>, Refused<p::Review>> {
        if !bounded(&input.scope().account_id, Some(&input.scope().key)) {
            return Err(refusal(
                &mut self.pending.lock().unwrap(),
                Reason::Metadata,
                input,
            ));
        }
        if input.row().state().pending().is_none() {
            return Err(refusal(
                &mut self.pending.lock().unwrap(),
                Reason::Stale,
                input,
            ));
        }
        let scope = input.scope().clone();
        self.step(&scope, input, "protected-pending-receipt", |i, reply| {
            Action::Receipt {
                expected: i.row().clone(),
                operation: i.row().state().pending().unwrap().clone(),
                reply,
            }
        })
    }
}
