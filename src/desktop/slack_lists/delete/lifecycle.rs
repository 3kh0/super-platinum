use super::*;

pub(super) fn validate(
    shell: &mut ShellState,
    attempt: &Attempt,
    loaded: &Loaded,
) -> Result<(), &'static str> {
    if attempt.restoring
        || !attempt.owned(shell)
        || !attempt.visible(shell)
        || shell
            .slack_lists
            .delete
            .receipts
            .get(&attempt.key())
            .is_none_or(|receipt| receipt.stage != Stage::Preparing)
        || create::blocks_mutation(shell)
        || shell
            .slack_lists
            .operations
            .contains_key(attempt.intent.list_id())
    {
        return Err("This deletion is out of date. Review the item before trying again.");
    }
    attempt
        .intent
        .revalidate(
            &loaded.document,
            &loaded.records,
            loaded.allow_lists.as_deref(),
            loaded.personal_task_lists.as_ref(),
        )
        .map_err(|e| e.0)?;
    shell
        .slack_lists
        .delete
        .receipts
        .get_mut(&attempt.key())
        .unwrap()
        .stage = Stage::Sending;
    Ok(())
}
pub(super) fn rejected(
    shell: &mut ShellState,
    attempt: &Attempt,
    message: &str,
    error: Option<&Error>,
) {
    if !attempt.owned(shell) {
        return;
    }
    shell.slack_lists.delete.receipts.remove(&attempt.key());
    if attempt.scope.current(shell)
        && let Some(view) =
            shell.slack_lists.view.as_mut().filter(|view| {
                view.id == attempt.intent.list_id() && view.generation == attempt.visit
            })
    {
        view.error = Some(message.into());
        view.retry_at = error.map(retry_at).unwrap_or(0);
    }
}
pub(super) fn finish(
    shell: &mut ShellState,
    attempt: &Attempt,
    result: Result<AcceptedDeletion, ListDeleteFailure>,
) -> bool {
    let result = result.and_then(|accepted| {
        if !attempt.restoring
            && accepted.list_id() == attempt.intent.list_id()
            && accepted.record_id() == attempt.intent.record_id()
        {
            Ok(())
        } else {
            Err(ListDeleteFailure::Uncertain(Error::Api(
                "Deletion receipt identity changed.".into(),
            )))
        }
    });
    finish_outcome(shell, attempt, result)
}
pub(super) fn finish_restore(
    shell: &mut ShellState,
    attempt: &Attempt,
    result: Result<AcceptedDeletionState, ListDeleteFailure>,
) -> bool {
    let result = result.and_then(|accepted| {
        if attempt.restoring
            && accepted.list_id() == attempt.intent.list_id()
            && accepted.record_id() == attempt.intent.record_id()
            && accepted.deleted() == attempt.redo
        {
            Ok(())
        } else {
            Err(ListDeleteFailure::Uncertain(Error::Api(
                "Restore receipt identity changed.".into(),
            )))
        }
    });
    finish_outcome(shell, attempt, result)
}
fn finish_outcome(
    shell: &mut ShellState,
    attempt: &Attempt,
    result: Result<(), ListDeleteFailure>,
) -> bool {
    if !attempt.owned(shell)
        || shell
            .slack_lists
            .delete
            .receipts
            .get(&attempt.key())
            .is_none_or(|receipt| {
                receipt.response_seen || !matches!(receipt.stage, Stage::Sending | Stage::Uncertain)
            })
    {
        return false;
    }
    if let Err(ListDeleteFailure::NotSent(error)) = &result {
        rejected(
            shell,
            attempt,
            "The change was not sent. Review the item before trying again.",
            Some(error),
        );
        return false;
    }
    let accepted = result.is_ok();
    let retry = result
        .as_ref()
        .err()
        .map(|failure| match failure {
            ListDeleteFailure::NotSent(error) | ListDeleteFailure::Uncertain(error) => {
                retry_at(error)
            }
        })
        .unwrap_or(0);
    let receipt = shell
        .slack_lists
        .delete
        .receipts
        .get_mut(&attempt.key())
        .unwrap();
    receipt.response_seen = true;
    receipt.stage = if accepted {
        Stage::AwaitingRead
    } else {
        Stage::Uncertain
    };
    receipt.retry_at = retry;
    // A late response may update only its retained original receipt. It cannot
    // invalidate or start reads in a replaced account, transport or visit.
    let visible = attempt.scope.current(shell)
        && shell.overlay == Some(Overlay::SlackList)
        && shell.slack_lists.view.as_ref().is_some_and(|view| {
            view.id == attempt.intent.list_id()
                && view.generation == attempt.visit
                && !view.unavailable
        });
    if visible {
        let view = shell.slack_lists.view.as_mut().unwrap();
        view.stale = true;
        view.revision = next_id();
    }
    visible && accepted && retry <= now()
}

pub(super) fn begin_check(shell: &mut ShellState, serial: u64) -> Option<ReadbackAttempt> {
    let key = key(shell)?;
    let receipt = shell.slack_lists.delete.receipts.get(&key)?;
    if receipt.attempt.serial != serial
        || receipt.read.is_some()
        || receipt.retry_at > now()
        || !matches!(receipt.stage, Stage::AwaitingRead | Stage::Uncertain)
        || shell.overlay != Some(Overlay::SlackList)
    {
        return None;
    }
    let view = shell.slack_lists.view.as_ref()?;
    if view.unavailable {
        return None;
    }
    let scope = shell.slack_lists.scope.clone()?;
    if !scope.current(shell) || !receipt.attempt.scope.stable_current(shell) {
        return None;
    }
    let read = ReadbackAttempt {
        key: key.clone(),
        serial,
        read: next_id(),
        scope,
        list_id: key.1.clone(),
        record_id: receipt.attempt.intent.record_id().into(),
        visit: view.generation,
    };
    let receipt = shell.slack_lists.delete.receipts.get_mut(&key).unwrap();
    receipt.read = Some(read.clone());
    receipt.message = Some(
        if receipt.attempt.restoring && !receipt.attempt.redo {
            "Checking whether this item is restored…"
        } else {
            "Checking whether this item is deleted…"
        }
        .into(),
    );
    Some(read)
}
impl ReadbackAttempt {
    pub(super) fn current(&self, shell: &ShellState) -> bool {
        self.scope.current(shell)
            && shell.overlay == Some(Overlay::SlackList)
            && shell.slack_lists.view.as_ref().is_some_and(|view| {
                view.id == self.list_id && view.generation == self.visit && !view.unavailable
            })
    }
    fn owns(&self, shell: &ShellState) -> bool {
        shell
            .slack_lists
            .delete
            .receipts
            .get(&self.key)
            .is_some_and(|receipt| {
                receipt.attempt.serial == self.serial
                    && receipt
                        .read
                        .as_ref()
                        .is_some_and(|read| read.read == self.read)
            })
    }
}
pub(super) fn finish_check(
    shell: &mut ShellState,
    read: &ReadbackAttempt,
    result: Result<ListRecordReadback, Error>,
) -> bool {
    if !read.owns(shell) {
        return false;
    }
    // Retire only this read lease even if its account or visit changed. An old
    // tombstone must not mutate the new visible context or replace a newer read.
    shell
        .slack_lists
        .delete
        .receipts
        .get_mut(&read.key)
        .unwrap()
        .read = None;
    if !read.current(shell) {
        return false;
    }
    let receipt = shell.slack_lists.delete.receipts.get(&read.key).unwrap();
    let restoring = receipt.attempt.restoring;
    let expected_deleted = !restoring || receipt.attempt.redo;
    let confirmed = result.as_ref().is_ok_and(|result| {
        result.list_id() == read.list_id
            && result.record_id() == read.record_id
            && if restoring {
                receipt
                    .restore_intent
                    .as_ref()
                    .is_some_and(|intent| intent.confirms_readback(result))
            } else {
                result.is_deleted()
            }
    });
    if !confirmed {
        let receipt = shell
            .slack_lists
            .delete
            .receipts
            .get_mut(&read.key)
            .unwrap();
        receipt.retry_at = receipt
            .retry_at
            .max(result.as_ref().err().map(retry_at).unwrap_or(0));
        receipt.message=Some(if restoring { "Could not confirm the item’s state. Check the result again; Undo or Redo will not be sent again." } else if result.as_ref().is_ok_and(|result|result.list_id()==read.list_id && result.record_id()==read.record_id && !result.is_deleted()) {
            "The item is still visible in Slack. Deletion remains unconfirmed; checking again will not resend it."
        } else {"Could not confirm deletion. Check the result again when the connection is available."}.into());
        return false;
    }
    // This exact readback also invalidates an older "Item added / Open item"
    // notice for the same owner and record, without touching other creations.
    if expected_deleted {
        create::reconcile_deleted(shell, &read.scope, result.as_ref().unwrap());
    }
    let receipt = shell.slack_lists.delete.receipts.remove(&read.key).unwrap();
    undo::retain(shell, &receipt.attempt);
    shell.slack_lists.delete.notice = Some((
        read.key.clone(),
        read.visit,
        if expected_deleted {
            "Item deleted."
        } else {
            "Item restored."
        }
        .into(),
    ));
    let close = expected_deleted
        && shell.slack_lists.view.as_ref().is_some_and(|view| {
            view.generation == receipt.attempt.visit
                && record_detail::matches_delete_origin(
                    view,
                    receipt.attempt.detail_open,
                    receipt.attempt.intent.record_id(),
                )
        });
    if close {
        record_detail::dismiss_shell(shell);
    }
    let view = shell.slack_lists.view.as_mut().unwrap();
    // Keep old content visibly stale until a normal full read refreshes it.
    // Never remove guessed cascade members from a filtered or incomplete page.
    view.stale = true;
    view.revision = next_id();
    true
}
