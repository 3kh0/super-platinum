use super::*;

/// Reserved synchronously before DOM capture. The predecessor is immutable
/// operation identity, not a request to read/claim a newer disk version.
pub(in crate::file_share) struct Capture {
    reservation: draft_io::Reservation,
    expected: Draft,
    pub predecessor: Option<Ticket>,
    pub prior_task: Option<Ticket>,
    pub visit: Arc<drafts::Visit>,
    pub ticket: Ticket,
    pub queue: Queue,
}
impl Capture {
    pub fn retain(
        self,
        note: Vec<Value>,
        destination: Option<Destination>,
    ) -> Result<Ticket, Refusal> {
        self.reservation
            .submit(Command::Retain {
                content: content(&self.expected, note, destination),
                reason: Refusal::Storage,
            })
            .map_err(|r| r.reason)
    }
    pub fn prepare(shell: &mut ShellState, id: &str) -> Result<Self, Refusal> {
        let dialog = visible(shell)
            .filter(|d| {
                d.id == id
                    && matches!(d.phase, Phase::Editing | Phase::Preparing)
                    && d.receipt.is_none()
            })
            .ok_or(Refusal::Changed)?;
        let expected = dialog.durable.clone().ok_or(Refusal::Changed)?;
        let visit = dialog
            .draft_visit
            .clone()
            .filter(|v| v.writable())
            .ok_or(Refusal::Changed)?;
        let storage = dialog.draft_storage.clone().ok_or(Refusal::Storage)?;
        // A new explicit Close/Share may retry a known storage refusal against
        // exactly the same expected version. It never observes/claims a newer
        // row, and the older captured operation remains separately retained.
        let prior_task = dialog.draft_task.clone();
        let predecessor=prior_task.clone().filter(|t| !matches!(
            (t.input_ref(),t.outcome_ref()),
            (Some(Command::Save{expected:old,..}|Command::Clear{expected:old,..}),Some(Outcome::Failed(Refusal::Storage))) if old==&expected));
        let queue = shell
            .file_share
            .custody
            .lock()
            .map_err(|_| Refusal::Storage)?
            .draft_io
            .clone();
        let reservation = queue.reserve(storage, visit.clone())?;
        let ticket = reservation.observe();
        shell.file_share.dialog.as_mut().unwrap().draft_task = Some(ticket.clone());
        Ok(Self {
            reservation,
            expected,
            predecessor,
            prior_task,
            visit,
            ticket,
            queue,
        })
    }
    pub async fn predecessor(&self) -> Result<Draft, Refusal> {
        let Some(prior) = &self.predecessor else {
            return Ok(self.expected.clone());
        };
        if !Arc::ptr_eq(prior.visit(), &self.visit) {
            return Err(Refusal::Changed);
        }
        let outcome = prior.wait().await;
        match (prior.input(), outcome) {
            (Some(Command::Save { expected, content }), Outcome::Saved(saved))
                if (expected == self.expected || saved == self.expected)
                    && saved.content == content =>
            {
                Ok(saved)
            }
            // No command was accepted and no disk operation occurred.
            (None, Outcome::Failed(_)) => Ok(self.expected.clone()),
            (_, Outcome::Blocked(_) | Outcome::Conflict(_)) => Err(Refusal::Changed),
            _ => Err(Refusal::Storage),
        }
    }
    pub async fn persist(
        self,
        note: Vec<Value>,
        destination: Option<Destination>,
        discard_empty: bool,
    ) -> Result<Ticket, Refusal> {
        let expected = self.predecessor().await;
        let value = content(&self.expected, note, destination);
        let command = match expected {
            Ok(expected) if discard_empty && value.empty() => Command::Clear {
                expected,
                content: value,
            },
            Ok(expected) => Command::Save {
                expected,
                content: value,
            },
            Err(reason) => Command::Retain {
                content: value,
                reason,
            },
        };
        // The original editor remains mounted on refusal. Input validation is
        // also bounded before submission; no refused command grants Close.
        self.reservation
            .submit(command)
            .map_err(|rejected| rejected.reason)
    }
}
