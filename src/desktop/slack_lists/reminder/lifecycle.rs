use super::*;

pub(super) fn loaded(
    shell: &mut ShellState,
    origin: &Origin,
    result: Result<(Loaded, Option<SavedItem>), Error>,
) {
    if shell
        .slack_lists
        .reminder
        .loads
        .remove(&origin.serial)
        .as_ref()
        != Some(&origin.key())
    {
        return;
    }
    if let Err(error) = &result {
        retain_cooldown(shell, origin.key(), error);
    }
    if !origin.owns_draft(shell) {
        return;
    }
    let current = origin.current(shell);
    let draft = shell.slack_lists.reminder.draft.as_mut().unwrap();
    draft.loading = false;
    if !current {
        draft.error = Some("This reminder visit changed. Close and reopen it.".into());
        return;
    }
    match result {
        Ok((loaded, item)) => {
            // Opening a draft must not adopt a newly replaced row or a changed
            // document while showing the old detail. A fresh explicit visit is required.
            let same = loaded
                .records
                .record(&origin.row)
                .is_some_and(|r| r.raw == origin.original)
                && shell
                    .slack_lists
                    .view
                    .as_ref()
                    .and_then(|v| v.loaded.as_ref())
                    .is_some_and(|old| old.document.raw == loaded.document.raw)
                && available_loaded(&loaded);
            if same {
                draft.loaded = Some(loaded);
                draft.saved = item;
            } else {
                draft.error =
                    Some("This item or your reminder access changed. Refresh it first.".into());
            }
        }
        Err(error) => {
            draft.retry_at = retry_at(&error);
            draft.error = Some(
                "Could not load the current reminder. Close and try again after refreshing.".into(),
            );
        }
    }
}
pub(super) fn begin_save(
    shell: &mut ShellState,
    serial: u64,
    change: ListReminderChange,
) -> Option<Attempt> {
    if capacity_used(shell) > 30 {
        return None;
    }
    let draft = shell.slack_lists.reminder.draft.as_ref()?;
    if draft.origin.serial != serial
        || !draft.origin.current(shell)
        || draft.loading
        || draft.retry_at > now()
        || cooldown(shell, &draft.origin.key()) > now()
        || shell
            .slack_lists
            .reminder
            .receipts
            .contains_key(&draft.origin.key())
    {
        return None;
    }
    let loaded = draft.loaded.as_ref()?;
    let intent = match ListReminderEdit::capture(
        &loaded.document,
        &loaded.records,
        &draft.origin.row,
        available_loaded(loaded),
        draft.saved.as_ref(),
        change,
    ) {
        Ok(intent) => intent,
        Err(error) => {
            shell.slack_lists.reminder.draft.as_mut()?.error = Some(error.0.into());
            return None;
        }
    };
    let attempt = Attempt {
        origin: draft.origin.clone(),
        intent,
    };
    shell
        .slack_lists
        .reminder
        .preflights
        .insert(attempt.origin.serial, attempt.origin.key());
    shell.slack_lists.reminder.receipts.insert(
        attempt.origin.key(),
        Receipt {
            attempt: attempt.clone(),
            phase: Phase::Preparing,
            response_seen: false,
            retry_at: 0,
            checking: None,
            message: "Checking the current reminder…".into(),
        },
    );
    Some(attempt)
}
impl Attempt {
    pub(super) fn owned(&self, shell: &ShellState) -> bool {
        shell
            .slack_lists
            .reminder
            .receipts
            .get(&self.origin.key())
            .is_some_and(|r| r.attempt.origin.serial == self.origin.serial)
    }
    pub(super) fn preparing(&self, shell: &ShellState) -> bool {
        self.owned(shell)
            && cooldown(shell, &self.origin.key()) <= now()
            && self.origin.current(shell)
            && self.origin.owns_draft(shell)
            && shell
                .slack_lists
                .reminder
                .receipts
                .get(&self.origin.key())
                .is_some_and(|r| r.phase == Phase::Preparing)
    }
    pub(super) fn sending(&self, shell: &ShellState) -> bool {
        self.owned(shell)
            && cooldown(shell, &self.origin.key()) <= now()
            && self.origin.current(shell)
            && self.origin.owns_draft(shell)
            && shell
                .slack_lists
                .reminder
                .receipts
                .get(&self.origin.key())
                .is_some_and(|r| r.phase == Phase::Sending && !r.response_seen)
    }
}
pub(super) fn validate(
    shell: &mut ShellState,
    attempt: &Attempt,
    loaded: &Loaded,
    item: Option<&SavedItem>,
) -> bool {
    if !attempt.preparing(shell) {
        return false;
    }
    if let Err(error) = attempt.intent.revalidate(
        &loaded.document,
        &loaded.records,
        available_loaded(loaded),
        item,
    ) {
        rejected(shell, attempt, error.0, None);
        return false;
    }
    let receipt = shell
        .slack_lists
        .reminder
        .receipts
        .get_mut(&attempt.origin.key())
        .unwrap();
    receipt.phase = Phase::Sending;
    receipt.message = "Saving reminder…".into();
    shell
        .slack_lists
        .reminder
        .preflights
        .remove(&attempt.origin.serial);
    true
}
pub(super) fn rejected(
    shell: &mut ShellState,
    attempt: &Attempt,
    message: &str,
    error: Option<&Error>,
) {
    shell
        .slack_lists
        .reminder
        .preflights
        .remove(&attempt.origin.serial);
    if let Some(error) = error {
        retain_cooldown(shell, attempt.origin.key(), error);
    }
    if !attempt.owned(shell) {
        return;
    }
    shell
        .slack_lists
        .reminder
        .receipts
        .remove(&attempt.origin.key());
    if attempt.origin.owns_draft(shell) {
        let draft = shell.slack_lists.reminder.draft.as_mut().unwrap();
        draft.error = Some(message.into());
        draft.retry_at = error.map(retry_at).unwrap_or(0);
    }
}
pub(super) fn finish(
    shell: &mut ShellState,
    attempt: &Attempt,
    result: Result<Value, ListReminderFailure>,
) -> bool {
    if !attempt.owned(shell)
        || shell
            .slack_lists
            .reminder
            .receipts
            .get(&attempt.origin.key())
            .is_none_or(|r| r.phase != Phase::Sending || r.response_seen)
    {
        return false;
    }
    if let Err(ListReminderFailure::NotRequested(error)) = &result {
        rejected(
            shell,
            attempt,
            "The reminder was not sent. Review it before trying again.",
            Some(error),
        );
        return false;
    }
    let accepted = result.as_ref().is_ok_and(|v| {
        v.get("ok") == Some(&Value::Bool(true)) && v.get("error").is_none_or(Value::is_null)
    });
    if let Err(ListReminderFailure::Unconfirmed(error)) = &result {
        retain_cooldown(shell, attempt.origin.key(), error);
    }
    let retry = result
        .as_ref()
        .err()
        .map(|error| match error {
            ListReminderFailure::NotRequested(e) | ListReminderFailure::Unconfirmed(e) => {
                retry_at(e)
            }
        })
        .unwrap_or(0);
    let receipt = shell
        .slack_lists
        .reminder
        .receipts
        .get_mut(&attempt.origin.key())
        .unwrap();
    receipt.phase = Phase::Uncertain;
    receipt.response_seen = true;
    receipt.retry_at = retry;
    receipt.attempt.origin.scope = receipt.attempt.origin.scope.receipt_only();
    receipt.message = if accepted {
        "Slack accepted the reminder. Checking its saved state…"
    } else {
        "The reminder result is not confirmed. Check the result; it will not be sent again."
    }
    .into();
    // Even a known API error is not a canonical saved-item observation. Keep
    // its receipt and never unlock by treating an acknowledgement as a row.
    accepted && attempt.origin.visible(shell) && retry <= now()
}
pub(super) fn begin_check(shell: &mut ShellState, serial: u64) -> Option<Read> {
    let key = key(shell)?;
    let receipt = shell.slack_lists.reminder.receipts.get(&key)?;
    if receipt.attempt.origin.serial != serial
        || receipt.phase != Phase::Uncertain
        || receipt.checking.is_some()
        || receipt.retry_at > now()
        || cooldown(shell, &key) > now()
        || shell.overlay != Some(Overlay::SlackList)
    {
        return None;
    }
    let scope = shell.slack_lists.scope.clone()?;
    if !scope.current(shell) || !receipt.attempt.origin.scope.stable_current(shell) {
        return None;
    }
    let read = Read {
        key: key.clone(),
        serial,
        token: next_id(),
        scope,
        visit: shell.slack_lists.view.as_ref()?.generation,
    };
    let receipt = shell.slack_lists.reminder.receipts.get_mut(&key)?;
    receipt.checking = Some(read.token);
    receipt.message = "Checking the saved reminder…".into();
    Some(read)
}
impl Read {
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        self.scope.current(shell)
            && cooldown(shell, &self.key) <= now()
            && shell.overlay == Some(Overlay::SlackList)
            && shell
                .slack_lists
                .view
                .as_ref()
                .is_some_and(|v| v.id == self.key.1 && v.generation == self.visit && !v.unavailable)
            && shell
                .slack_lists
                .reminder
                .receipts
                .get(&self.key)
                .is_some_and(|r| {
                    r.attempt.origin.serial == self.serial && r.checking == Some(self.token)
                })
    }
}
pub(super) fn finish_check(
    shell: &mut ShellState,
    read: &Read,
    result: Result<Loaded, Error>,
) -> bool {
    let current = read.current(shell);
    let owned = shell
        .slack_lists
        .reminder
        .receipts
        .get(&read.key)
        .is_some_and(|r| r.attempt.origin.serial == read.serial && r.checking == Some(read.token));
    if !owned {
        return false;
    }
    if let Err(error) = &result {
        retain_cooldown(shell, read.key.clone(), error);
    }
    let Some(receipt) = shell
        .slack_lists
        .reminder
        .receipts
        .get_mut(&read.key)
        .filter(|r| r.attempt.origin.serial == read.serial && r.checking == Some(read.token))
    else {
        return false;
    };
    receipt.checking = None;
    if !current {
        return false;
    }
    let confirmed = result.as_ref().is_ok_and(|loaded| {
        receipt
            .attempt
            .intent
            .readback_matches(&loaded.document, &loaded.records)
    });
    if !confirmed {
        receipt.retry_at = receipt
            .retry_at
            .max(result.as_ref().err().map(retry_at).unwrap_or(0));
        receipt.message="The saved reminder does not yet confirm this change. Check again; this will not resend it.".into();
        return false;
    }
    shell.slack_lists.reminder.receipts.remove(&read.key);
    if shell
        .slack_lists
        .reminder
        .draft
        .as_ref()
        .is_some_and(|d| d.origin.serial == read.serial)
    {
        shell.slack_lists.reminder.draft = None;
    }
    // Preserve all displayed raw records/cursors until the ordinary guarded
    // refresh installs a complete canonical projection.
    if let Some(view) = shell.slack_lists.view.as_mut() {
        view.stale = true;
        view.error = Some("Reminder updated.".into());
    }
    true
}
