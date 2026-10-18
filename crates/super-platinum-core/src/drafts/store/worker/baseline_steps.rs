//! Baseline completion acknowledgment and explicit durable settlement.
use super::family_retirement::{keys, transferred};
use super::*;
use crate::drafts::store::worker::owned_queue::operations::BaselineFinishInput;

impl Worker {
    pub(in crate::drafts::store::worker) fn retry_finish_baseline(
        &self,
        step: &Step<BaselineFinishInput, p::Review>,
    ) -> Result<(), Reason> {
        let input = step.input();
        let (reply, receiver) = mpsc::channel();
        let request = wrapped(
            step.key.route.owner.account(),
            Action::FinishBaseline {
                evidence: input.evidence().clone(),
                choice: input.choice().clone(),
                reply,
            },
        );
        let pending = self.pending.lock().map_err(|_| Reason::Incomplete)?;
        let a = pending.admission.as_ref().ok_or(Reason::Unsupported)?;
        let entry = a.entry(&step.key)?;
        let actual = entry
            .retained
            .clone()
            .downcast::<StepHeld<BaselineFinishInput, p::Review>>()
            .map_err(|_| Reason::Stale)?;
        if !Arc::ptr_eq(&actual, &step.held) || entry.generation != a.generation {
            return Err(Reason::Stale);
        }
        a.check(step.key.route.owner.account(), false)?;
        let mut s = actual.status.lock().map_err(|_| Reason::Incomplete)?;
        if !matches!(s.result, Some(Err(p::Error::Storage))) {
            return Err(Reason::Stale);
        }
        let id = s.submitted.ok_or(Reason::Incomplete)?;
        if pending
            .accounting
            .as_ref()
            .and_then(|b| b.terminal(id))
            .is_none()
        {
            return Err(Reason::Incomplete);
        }
        s.failures.push((id, p::Error::Storage));
        s.result = None;
        s.submitted = None;
        s.receiver = Some(receiver);
        s.request = Some(request);
        drop(s);
        drop(pending);
        self.submit_step(step)
    }
    pub(in crate::drafts::store::worker) fn hold_finish_baseline(
        &self,
        input: BaselineFinishInput,
    ) -> Result<Step<BaselineFinishInput, p::Review>, Refused<BaselineFinishInput>> {
        let scope = input.evidence().scope().clone();
        self.step(&scope, input, "protected-finish-baseline", |i, reply| {
            Action::FinishBaseline {
                evidence: i.evidence().clone(),
                choice: i.choice().clone(),
                reply,
            }
        })
    }
    pub(in crate::drafts::store::worker) fn observe_baseline_family(
        &self,
        c: &Completion,
        expected: Option<&Arc<Finished>>,
        retire: bool,
    ) -> Result<Arc<Finished>, Reason> {
        self.observe_baseline_retirement(c, expected, None, false, retire)
            .map(|v| v.0)
    }
    pub(in crate::drafts::store::worker) fn observe_baseline_retirement(
        &self,
        c: &Completion,
        expected: Option<&Arc<Finished>>,
        expected_reports: Option<&[(u64, terminal::Report)]>,
        repair: bool,
        retire: bool,
    ) -> Result<(Arc<Finished>, Vec<(u64, terminal::Report)>), Reason> {
        c.result()?;
        let mut pending = self.pending.lock().map_err(|_| Reason::Incomplete)?;
        let keys = keys(
            &pending,
            &c.key,
            &[
                "protected-reserve-baseline",
                "protected-claim-baseline",
                "protected-retain-baseline",
            ],
        )?;
        let actual = pending
            .admission
            .as_ref()
            .unwrap()
            .entry(&c.key)?
            .retained
            .clone()
            .downcast::<CompletionHeld>()
            .map_err(|_| Reason::Stale)?;
        if !Arc::ptr_eq(&actual, &c.held) {
            return Err(Reason::Stale);
        }
        let s = actual.status.lock().map_err(|_| Reason::Incomplete)?;
        if !matches!(s.result.as_deref(), Some(Finished::Baseline(Ok(_))))
            || !matches!(s.raw.as_deref(), Some(Raw::Baseline { .. }))
            || !matches!(s.binding.as_deref(), Some(Binding::Baseline(_)))
            || !s.baseline
            || s.building
            || s.disconnected
            || s.issued.is_some()
            || s.dispatch.is_some()
            || s.request.is_some()
            || if repair {
                s.attempts.len() < 2
            } else {
                s.attempts.len() != 1
            }
            || !s.deferred.settled()
        {
            return Err(Reason::Incomplete);
        }
        let id = s.submitted.ok_or(Reason::Incomplete)?;
        let observed = s.result.as_ref().unwrap().clone();
        let last = s.attempts.last().ok_or(Reason::Incomplete)?;
        if last.0 != id
            || !Arc::ptr_eq(&last.1, &observed)
            || s.attempts[..s.attempts.len() - 1]
                .iter()
                .any(|(_, result)| {
                    !matches!(result.as_ref(), Finished::Baseline(Err(p::Error::Storage)))
                })
        {
            return Err(Reason::Incomplete);
        }
        if retire && expected.is_none_or(|v| !Arc::ptr_eq(v, &observed)) {
            return Err(Reason::Stale);
        }
        let mut ids = vec![
            transferred::<BaselineInput, p::BaselineTicket>(&pending, &keys[0], &keys[1])?,
            transferred::<Arc<BaselineInput>, p::BaselineClaim>(&pending, &keys[1], &keys[2])?,
        ];
        // Each explicit same-ticket re-claim adds one actual terminal record,
        // never another family slot. All are checked and retired together.
        ids.extend(s.deferred.history.iter().map(|(id, _)| *id));
        let issuance_count = ids.len();
        ids.extend(s.attempts.iter().map(|(id, _)| *id));
        let book = pending.accounting.as_ref().ok_or(Reason::Incomplete)?;
        let reports = ids
            .iter()
            .map(|id| {
                book.terminal(*id)
                    .map(|r| (*id, r))
                    .ok_or(Reason::Incomplete)
            })
            .collect::<Result<Vec<_>, _>>()?;
        // Every intermediate claim returned the same ticket, followed by one
        // actual Ready. Errors or unresolved claims cannot be acknowledged.
        if s.baseline_deferred_initial != !s.deferred.history.is_empty()
            || s.deferred
                .history
                .iter()
                .enumerate()
                .any(|(i, (_, result))| {
                    if i + 1 == s.deferred.history.len() {
                        !matches!(result, Ok(None))
                    } else {
                        !matches!(result, Ok(Some(_)))
                    }
                })
        {
            return Err(Reason::Incomplete);
        }
        if expected_reports.is_some_and(|expected| expected != reports.as_slice())
            || (repair && retire && expected_reports.is_none())
        {
            return Err(Reason::Stale);
        }
        let book = pending.accounting.as_mut().unwrap();
        let accounted = if repair {
            book.baseline_repaired_family(&reports, issuance_count, retire)
        } else {
            book.baseline_family(&reports, retire)
        };
        if !accounted {
            return Err(Reason::Incomplete);
        }
        if retire {
            for key in keys {
                pending.admission.as_mut().unwrap().slots.remove(&key.id);
            }
        }
        Ok((observed, reports))
    }
}
#[path = "baseline_terminal_steps.rs"]
mod terminal_steps;
