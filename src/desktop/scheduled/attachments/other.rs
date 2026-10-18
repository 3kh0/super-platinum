//! Shared reservations for preexisting text editing, send, delete and unschedule.
//! Detaching a dialog is not completion of its retained async write.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    Edit,
    Copy,
    Unschedule,
    Send,
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    Idle,
    Running,
    Uncertain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// A read-only preflight failed, or the transport proved no dispatch occurred.
    NotDispatched,
    /// A complete, narrowly recognized explicit server rejection; no replay.
    Rejected,
    Confirmed,
    /// The owning durable workflow explicitly retained both versions and chose
    /// a freshly verified conflict baseline. This is not a success/rejection.
    ReviewedConflict,
    Unknown,
}

#[derive(Debug)]
pub(super) struct Other {
    attached: bool,
    phase: Phase,
    action: Action,
    expected: Option<ScheduleMessage>,
    reviewing: bool,
    review_after: Option<(Instant, Duration)>,
}

impl Default for Other {
    fn default() -> Self {
        Self {
            attached: true,
            phase: Phase::Idle,
            action: Action::Edit,
            expected: None,
            reviewing: false,
            review_after: None,
        }
    }
}

impl Other {
    pub(super) fn idle(&self) -> bool {
        self.phase == Phase::Idle
    }
    pub(super) fn uncertain(&self) -> bool {
        self.phase == Phase::Uncertain
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Snapshot {
    pub action: Action,
    pub phase: Phase,
    pub reviewing: bool,
    pub can_confirm_by_readback: bool,
    pub cooling_down: bool,
}

impl Operations {
    /// Reserve at editor open. Call begin_other before the first save await, so
    /// editor unmount cannot release the reservation while preflight is running.
    /// Expected payload may be supplied after reading the rich editor snapshot.
    pub fn begin_other(&mut self, lease: &Lease, action: Action) -> Result<(), Fault> {
        let other = self.other_mut(lease)?;
        if !other.idle() {
            return Err(Fault::Retained);
        }
        other.phase = Phase::Running;
        other.action = action;
        other.expected = None;
        Ok(())
    }

    pub fn expect_other(&mut self, lease: &Lease, expected: ScheduleMessage) -> Result<(), Fault> {
        let other = self.other_mut(lease)?;
        if other.phase != Phase::Running
            || other.expected.is_some()
            || !matches!(other.action, Action::Edit | Action::Unschedule)
        {
            return Err(Fault::Retained);
        }
        other.expected = Some(expected);
        Ok(())
    }

    pub fn detach_other(&mut self, lease: &Lease) -> Result<(), Fault> {
        let other = self.other_mut(lease)?;
        if other.idle() {
            self.entries.remove(&lease.owner);
        } else {
            other.attached = false;
        }
        Ok(())
    }

    /// Always settle the original stable lease, even after account/route changes.
    /// A parent may update the current queue only after separate Context admission.
    pub fn settle_other(&mut self, lease: &Lease, outcome: Outcome) -> Result<(), Fault> {
        let other = self.other_mut(lease)?;
        if other.phase != Phase::Running {
            return Err(Fault::Retained);
        }
        match outcome {
            Outcome::Unknown => other.phase = Phase::Uncertain,
            Outcome::NotDispatched
            | Outcome::Rejected
            | Outcome::Confirmed
            | Outcome::ReviewedConflict => {
                if other.attached {
                    other.phase = Phase::Idle;
                    other.expected = None;
                } else {
                    self.entries.remove(&lease.owner);
                }
            }
        }
        Ok(())
    }

    pub fn other_snapshot(&self, lease: &Lease) -> Result<Snapshot, Fault> {
        let other = self.other(lease)?;
        Ok(Snapshot {
            action: other.action,
            phase: other.phase,
            reviewing: other.reviewing,
            can_confirm_by_readback: other.expected.is_some()
                && matches!(other.action, Action::Edit | Action::Unschedule),
            cooling_down: other
                .review_after
                .is_some_and(|(started, duration)| started.elapsed() < duration),
        })
    }

    /// Discover uncertainty even when the server queue no longer lists the draft.
    pub fn other_retained(&self, shell: &ShellState) -> Vec<Lease> {
        self.entries.values().filter_map(|entry| {
            matches!(&entry.reservation, Reservation::Other(other) if other.phase != Phase::Idle)
                .then_some(&entry.lease).filter(|lease| lease.matches_shell(shell)).cloned()
        }).collect()
    }

    pub fn begin_other_review(&mut self, lease: &Lease) -> Result<(), Fault> {
        let other = self.other_mut(lease)?;
        if other.phase != Phase::Uncertain || other.reviewing {
            return Err(Fault::Retained);
        }
        if other
            .review_after
            .is_some_and(|(started, duration)| started.elapsed() < duration)
        {
            return Err(Fault::CoolingDown);
        }
        other.reviewing = true;
        Ok(())
    }

    /// Positive full readback can settle an edit/unschedule only. Send/delete
    /// absence or flags are not proven terminal outcomes in this protocol slice.
    pub fn finish_other_review(
        &mut self,
        lease: &Lease,
        response: Option<&Value>,
    ) -> Result<bool, Fault> {
        let other = self.other_mut(lease)?;
        if other.phase != Phase::Uncertain || !other.reviewing {
            return Err(Fault::Retained);
        }
        other.reviewing = false;
        let confirmed = matches!(other.action, Action::Edit | Action::Unschedule)
            && other.expected.as_ref().is_some_and(|expected| {
                response.is_some_and(|response| {
                    super_platinum_core::scheduled::edit::confirms_submission(
                        response,
                        &lease.owner.draft,
                        expected,
                    )
                })
            });
        if confirmed {
            if other.attached {
                other.phase = Phase::Idle;
                other.expected = None;
            } else {
                self.entries.remove(&lease.owner);
            }
        }
        Ok(confirmed)
    }

    pub fn other_review_cooldown(&mut self, lease: &Lease, seconds: u64) -> Result<(), Fault> {
        self.other_mut(lease)?.review_after = Some((Instant::now(), Duration::from_secs(seconds)));
        Ok(())
    }

    pub fn refresh_other_cooldown(&mut self, lease: &Lease) -> Result<(), Fault> {
        let other = self.other_mut(lease)?;
        if other
            .review_after
            .is_some_and(|(started, duration)| started.elapsed() >= duration)
        {
            other.review_after = None;
        }
        Ok(())
    }

    pub(super) fn other(&self, lease: &Lease) -> Result<&Other, Fault> {
        match &self.entry(lease)?.reservation {
            Reservation::Other(other) => Ok(other),
            Reservation::Attachments(_) => Err(Fault::Retained),
        }
    }
    fn other_mut(&mut self, lease: &Lease) -> Result<&mut Other, Fault> {
        self.entry(lease)?;
        match &mut self
            .entries
            .get_mut(&lease.owner)
            .ok_or(Fault::StaleLease)?
            .reservation
        {
            Reservation::Other(other) => Ok(other),
            Reservation::Attachments(_) => Err(Fault::Retained),
        }
    }
}

#[cfg(test)]
mod tests;
