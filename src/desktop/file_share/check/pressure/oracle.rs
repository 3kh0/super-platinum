use super::*;
#[derive(Default)]
pub(super) struct Oracle {
    claim: Option<Draft>,
    saved: Option<Draft>,
    owner: String,
    visit: Option<Arc<drafts::Visit>>,
    pub failures: Vec<Ticket>,
    pub records: Vec<Value>,
}
impl Oracle {
    pub fn failed(
        &mut self,
        shell: &ShellState,
        ticket: Ticket,
        index: usize,
        snapshot: &durable_draft::Snapshot,
    ) -> Result<Value, String> {
        let claim = self.claim.as_ref().ok_or("Pressure initial claim absent")?;
        let dialog = visible(shell).ok_or("Failed Close removed editor")?;
        let Some(Command::Save { expected, content }) = ticket.input() else {
            return Err("Pressure Close did not submit Save".into());
        };
        if &expected != claim
            || content.source != claim.content.source
            || content.title != claim.content.title
            || json!(content.note) != earlier(index)
            || !destination(&content.destination)
            || !matches!(
                ticket.outcome_ref(),
                Some(Outcome::Failed(Refusal::Storage))
            )
            || dialog.error
                != Some("The message could not be saved. Keep this window open and try again.")
            || !self.same(dialog)
            || !dialog.draft_task.as_ref().is_some_and(|t| t.same(&ticket))
            || dialog.durable.as_ref() != Some(claim)
            || durable_draft::one(snapshot)? != claim
            || self.failures.iter().any(|old| old.same(&ticket))
        {
            return Err("Failed Close did not retain exact indexed input/original CAS".into());
        }
        let record = json!({"index":index,"id":ticket.id(),"command":"save","outcome":"storage","expected":expected,"content":content,"sameVisit":true});
        self.failures.push(ticket);
        self.records.push(record.clone());
        self.retained(shell, self.failures.len())?;
        Ok(record)
    }
    fn same(&self, dialog: &Dialog) -> bool {
        dialog.id == self.owner
            && dialog.phase == Phase::Editing
            && !dialog.closing
            && !dialog.draft_local
            && dialog.receipt.is_none()
            && dialog
                .draft_visit
                .as_ref()
                .zip(self.visit.as_ref())
                .is_some_and(|(a, b)| Arc::ptr_eq(a, b))
    }
    fn retained(&self, shell: &ShellState, count: usize) -> Result<(), String> {
        let held = shell
            .file_share
            .custody
            .lock()
            .map_err(|_| "Pressure custody poisoned")?;
        let tickets = held
            .draft_io
            .retained()
            .map_err(|_| "Pressure queue poisoned")?;
        let observation = held
            .draft_io
            .diagnostic()
            .map_err(|_| "Pressure observation poisoned")?;
        if !held.pending.is_empty()
            || !held.drafts.is_empty()
            || tickets.len() != count
            || observation.jobs != count
            || observation.scopes != 0
            || observation.running != 0
            || self
                .failures
                .iter()
                .take(count)
                .any(|old| !tickets.iter().any(|now| now.same(old)))
        {
            return Err(
                "Pressure retained queue contains wrong operation or unfinished I/O".into(),
            );
        }
        Ok(())
    }
    pub fn stage(
        &mut self,
        shell: &ShellState,
        stage: &str,
        ui: &Value,
        snapshot: &durable_draft::Snapshot,
    ) -> Result<Value, String> {
        let row = durable_draft::one(snapshot)?;
        if stage == "ready" {
            let d = visible(shell).ok_or("Pressure ready dialog absent")?;
            if row.revision != 1
                || !row.content.empty()
                || d.durable.as_ref() != Some(row)
                || d.phase != Phase::Editing
                || d.closing
                || d.receipt.is_some()
                || d.draft_task.is_some()
                || ui["owner"] != d.id
                || ui["note"] != earlier(0)
                || !d.destination.as_ref().is_some_and(|d| {
                    d.id == fixture::DESTINATION && d.name == "share-fixture" && !d.private
                })
            {
                return Err("Pressure ready did not claim empty original version".into());
            }
            self.claim = Some(row.clone());
            self.owner = d.id.clone();
            self.visit = d.draft_visit.clone();
            if self
                .visit
                .as_ref()
                .is_none_or(|v| !v.active() || !v.writable())
            {
                return Err("Pressure Visit absent".into());
            }
            self.retained(shell, 0)?;
        } else if matches!(stage, "full" | "capacity" | "preview" | "discarded") {
            let d = visible(shell).ok_or("Pressure editor disappeared")?;
            let count = if stage == "discarded" { 31 } else { 32 };
            if self.failures.len() != 32
                || !self.same(d)
                || d.durable.as_ref() != self.claim.as_ref()
                || self.claim.as_ref() != Some(row)
                || ui["note"] != current_note()
                || ui["retained"]["ok"] != true
                || ui["owner"] != self.owner
            {
                return Err("Pressure controls changed current editor or original claim".into());
            }
            self.retained(shell, count)?;
            if stage == "discarded" {
                if d.draft_task.is_some()
                    || ui["undoRedo"]["ok"] != true
                    || ui["undoRedo"]["beforeUndo"]["ok"] != true
                    || ui["undoRedo"]["undoNote"] != earlier(31)
                    || ui["undoRedo"]["redoNote"] != current_note()
                {
                    return Err(
                        "Explicit earlier discard missed exact task or undo transaction".into(),
                    );
                }
            } else if !d
                .draft_task
                .as_ref()
                .is_some_and(|t| t.same(&self.failures[31]))
            {
                return Err("Current earlier ticket changed".into());
            }
            if stage == "capacity" && ui["capacityAttempt"] != true {
                return Err("No actual Close under full queue".into());
            }
            if stage == "preview"
                && (ui["previewText"] != "Earlier capture 31"
                    || ui["previewDestination"] != "To share-fixture"
                    || ui["previewId"] != self.failures[31].id())
            {
                return Err("Preview differs from exact earlier copy".into());
            }
        } else if stage == "closed" {
            if !durable_draft::saved_from(
                self.claim.as_ref().ok_or("No claim")?,
                row,
                &current_note(),
            ) || visible(shell).is_some()
                || shell.overlay.is_some()
                || ui["returnedFocus"] != true
                || ui["oldDetached"] != true
                || ui["owner"] != self.owner
            {
                return Err("Pressure Close hid before exact committed current input".into());
            }
            self.retained(shell, 31)?;
            self.saved = Some(row.clone());
        } else if stage == "reopened" {
            let d = visible(shell).ok_or("Pressure reopened dialog absent")?;
            if !durable_draft::reclaimed(
                self.saved.as_ref().ok_or("Pressure committed row absent")?,
                row,
            ) || d.durable.is_some()
                || d.phase != Phase::DraftReview
                || d.closing
                || d.receipt.is_some()
                || !d.editor_ready
                || !d.draft_local
                || !d.saved_draft_stale
                || d.draft_task
                    .as_ref()
                    .zip(self.failures.get(30))
                    .is_none_or(|(a, b)| !a.same(b))
                || d.error
                    != Some(
                        "This captured message has not been saved. Review it before discarding it.",
                    )
                || d.id == self.owner
                || ui["owner"] != d.id
                || ui["note"] != earlier(30)
                || json!(d.note) != earlier(30)
                || ui["readonly"] != true
                || ui["retainedReview"] != true
                || !d.destination.as_ref().is_some_and(|d| {
                    d.id == fixture::DESTINATION && d.name == "share-fixture" && !d.private
                })
                || d.draft_visit
                    .as_ref()
                    .zip(self.visit.as_ref())
                    .is_none_or(|(a, b)| !Arc::ptr_eq(a, b))
            {
                return Err(
                    "Pressure reopen lost exact retained review or separate committed claim".into(),
                );
            }
            self.retained(shell, 31)?;
        } else {
            return Err("Unknown pressure stage".into());
        }
        Ok(
            json!({"retained":if matches!(stage,"discarded"|"closed"|"reopened"){31}else if stage=="ready"{0}else{32},"operations":self.records,"pending":0,"processDrafts":0,"scopes":0,"running":0,"originalOwner":self.owner,"originalClaim":self.claim,"unchangedOtherCaptures":true}),
        )
    }
}
fn destination(
    value: &Option<super_platinum_core::file_sharing::store::drafts::Destination>,
) -> bool {
    value
        .as_ref()
        .is_some_and(|d| d.id == fixture::DESTINATION && d.name == "share-fixture" && !d.private)
}
