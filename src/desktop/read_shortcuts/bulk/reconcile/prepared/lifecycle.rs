use super::*;

pub(super) fn start_clear(s: &mut ShellState, c: Arc<Context>, confirmed: bool) -> Option<Work> {
    if !c.visit_current(s)
        || cursor_busy(s, &c.owner.team)
        || s.bulk_read
            .entries
            .get(&c.owner)
            .is_some_and(|e| e.operation.is_some() || e.reconciling.is_some())
        || (!confirmed
            && (observed(s, &c).requires_confirmation() != Ok(false)
                || preference_pending(s, &c.owner)))
    {
        return None;
    }
    let request = api::BulkReadIntent::clear(c.api_owner.clone())
        .request(&c.client, &c.session, c.epoch)
        .ok()?;
    Some(reserve(s, c, Kind::Clear, request))
}
fn reserve(s: &mut ShellState, c: Arc<Context>, kind: Kind, request: PreparedRequest) -> Work {
    let id = next_id();
    let baseline = Arc::new(reconcile::Baseline::capture(s, &c));
    let e = s.bulk_read.entries.entry(c.owner.clone()).or_default();
    e.notice = None;
    e.hidden = false;
    e.refresh_failed = false;
    e.acknowledged = false;
    e.message = Some(
        if kind == Kind::Clear {
            "Clearing unread messages and notifications…"
        } else {
            "Restoring unread state…"
        }
        .into(),
    );
    e.operation = Some(Operation {
        id,
        context: c.clone(),
        kind,
        phase: Phase::Sending,
    });
    Work {
        id,
        context: c,
        kind,
        request,
        baseline,
    }
}
pub(super) fn begin_undo(s: &mut ShellState, now: Instant) -> Option<Work> {
    let owner = Owner::capture(s)?;
    let e = s.bulk_read.entries.get(&owner)?;
    let n = e.notice.as_ref()?;
    if e.operation.is_some()
        || now.duration_since(n.started) >= n.duration
        || !n.context.current(s)
        || cursor_busy(s, &owner.team)
    {
        return None;
    }
    let c = Arc::new(Context::capture(s).ok()?);
    if !c.visit_current(s) {
        return None;
    }
    let receipt = s
        .bulk_read
        .entries
        .get_mut(&owner)?
        .notice
        .take()?
        .receipt?;
    // Undo has a fixed six-second source window. Retire the older readback
    // generation rather than spending that window waiting for network reads.
    s.bulk_read.entries.get_mut(&owner)?.reconciling = None;
    let request = receipt
        .into_undo()?
        .request(&c.client, &c.session, c.epoch)
        .ok()?;
    Some(reserve(s, c, Kind::Undo, request))
}
pub(super) fn sending(s: &ShellState, w: &Work) -> bool {
    w.context.visit_current(s)
        && s.bulk_read
            .entries
            .get(&w.context.owner)
            .and_then(|e| e.operation.as_ref())
            .is_some_and(|o| o.id == w.id && o.phase == Phase::Sending)
}
pub(super) fn cancel_unsent(s: &mut ShellState, w: &Work) {
    if let Some(e) = s
        .bulk_read
        .entries
        .get_mut(&w.context.owner)
        .filter(|e| e.operation.as_ref().is_some_and(|o| o.id == w.id))
    {
        e.operation = None;
        e.message =
            Some("Nothing was changed. The workspace changed before the request was sent.".into());
    }
}
pub(crate) fn finish_write(
    s: &mut ShellState,
    w: &Work,
    result: Result<serde_json::Value, Error>,
) -> Option<u64> {
    let current = w.context.current(s);
    let result =
        result.and_then(|raw| api::BulkReadReceipt::parse(w.context.api_owner.clone(), &raw));
    let e = s.bulk_read.entries.get_mut(&w.context.owner)?;
    if !e
        .operation
        .as_ref()
        .is_some_and(|o| o.id == w.id && o.phase == Phase::Sending)
    {
        return None;
    }
    match result {
        Ok(receipt) => {
            e.operation = None;
            e.acknowledged = true;
            e.message = Some(
                if w.kind == Kind::Undo {
                    "Unread state restored."
                } else {
                    "Unread messages and notifications cleared."
                }
                .into(),
            );
            if !current {
                e.message = Some("The request succeeded in the previous workspace. Refresh its read state when you return.".into());
                return None;
            }
            let has_undo = w.kind == Kind::Clear && receipt.has_undo();
            e.notice = Some(Notice {
                context: w.context.clone(),
                receipt: has_undo.then_some(receipt),
                started: Instant::now(),
                duration: Duration::from_secs(if has_undo { 6 } else { 3 }),
                undone: w.kind == Kind::Undo,
            });
            let id = next_id();
            e.reconciling = Some(id);
            Some(id)
        }
        Err(error) if api::read_shortcut_definitely_rejected(&error) => {
            e.operation = None;
            e.message = Some("Nothing was changed. Slack did not accept the request.".into());
            None
        }
        Err(_) => {
            e.operation.as_mut()?.phase = Phase::Unknown;
            e.message = Some("The outcome is unconfirmed. Refresh read state to see current counts. This request will not be sent again.".into());
            None
        }
    }
}
pub(super) fn start_reconcile(s: &mut ShellState, c: &Context) -> Option<u64> {
    if !c.current(s) {
        return None;
    }
    let e = s.bulk_read.entries.get_mut(&c.owner)?;
    if e.reconciling.is_some()
        || e.operation
            .as_ref()
            .is_some_and(|o| o.phase == Phase::Sending)
    {
        return None;
    }
    let id = next_id();
    e.reconciling = Some(id);
    Some(id)
}
pub(crate) fn finish_reconcile(s: &mut ShellState, c: &Context, id: u64, success: bool) {
    let current = c.current(s);
    if let Some(e) = s
        .bulk_read
        .entries
        .get_mut(&c.owner)
        .filter(|e| e.reconciling == Some(id))
    {
        e.reconciling = None;
        e.refresh_failed = !success || !current;
        // Counts cannot prove an uncertain mutation did or did not execute.
        if e.operation.is_none() {
            if e.refresh_failed {
                e.message = Some(if e.acknowledged {"The request succeeded, but current read state could not be refreshed. Refresh when connected."} else {"Current read state could not be refreshed. Refresh when connected."}.into());
            } else {
                e.message = e.notice.as_ref().map(|notice| {
                    if notice.undone {
                        "Unread state restored."
                    } else {
                        "Unread messages and notifications cleared."
                    }
                    .into()
                });
            }
        }
    }
}
