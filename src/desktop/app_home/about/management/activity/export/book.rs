use super::*;
pub(super) const MAX_EXPORTS: usize = 128;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Phase {
    Preparing,
    Sent,
    NotSent,
    Rejected,
    Unknown,
    Initiated,
}
impl Phase {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Preparing => "preparing",
            Self::Sent => "sent",
            Self::NotSent => "not-sent",
            Self::Rejected => "rejected",
            Self::Unknown => "unknown",
            Self::Initiated => "initiated",
        }
    }
    pub(super) fn notice(self) -> &'static str {
        match self {
            Self::Preparing => "Checking the current filters before exporting\u{2026}",
            Self::Sent => "The export request is being sent.",
            Self::NotSent => "The export request was not sent. Your draft is unchanged.",
            Self::Rejected => "Slack did not accept the export request.",
            Self::Unknown => {
                "The export request may have reached Slack. It has not been sent again."
            }
            Self::Initiated => {
                "The export request was accepted. The CSV has not been confirmed ready yet."
            }
        }
    }
}
pub(in crate::app_home::about::management) struct Attempt {
    pub(super) id: u64,
    pub(super) ticket: Ticket,
    pub(super) phase: Phase,
    pub(super) holder: Option<ActivityExport>,
    pub(super) request: Option<PreparedRequest>,
    pub(super) receipt: Option<Result<Value, Error>>,
    pub(super) initiated: Option<ActivityExportInitiated>,
}
impl Attempt {
    pub(super) fn matches(&self, d: &Document) -> bool {
        self.ticket.document == d.id
            && self.ticket.account == d.account
            && self.ticket.owner.ctx.session.team_id == d.team
            && self.ticket.owner.ctx.session.user_id == d.user
            && self.ticket.workflow == d.editor.workflow_id()
    }
}
pub(super) fn same_filters(a: &Filters, b: &Filters) -> bool {
    // Source export omits all uppercase UI statuses, page size and parent scope.
    a.user == b.user && a.from == b.from && a.to == b.to
}
pub(super) fn blocked(s: &ShellState, d: &Document) -> bool {
    s.app_home.about.management.exports.iter().any(|a| {
        a.matches(d)
            && (matches!(a.phase, Phase::Preparing | Phase::Sent)
                || a.phase == Phase::Unknown
                    && same_filters(&a.ticket.filters, &d.activity.filters))
    })
}
pub(super) fn reserve(s: &mut ShellState, ticket: &Ticket) -> Option<read::Lease> {
    if !ticket.current(s) || s.app_home.about.management.exports.len() >= MAX_EXPORTS {
        return None;
    }
    let d = document(s, ticket.visit)?;
    if blocked(s, d) {
        return None;
    }
    let lease = read::Lease::begin(s, ticket.visit)?;
    s.app_home.about.management.exports.push(Attempt {
        id: lease.id,
        ticket: ticket.clone(),
        phase: Phase::Preparing,
        holder: None,
        request: None,
        receipt: None,
        initiated: None,
    });
    Some(lease)
}
pub(super) fn claim(
    s: &mut ShellState,
    id: u64,
    mut holder: ActivityExport,
) -> Option<PreparedRequest> {
    let a = s
        .app_home
        .about
        .management
        .exports
        .iter()
        .find(|a| a.id == id)?;
    if a.phase != Phase::Preparing
        || !a.ticket.current(s)
        || s.app_home.about.management.surface.as_ref()?.read != Some(id)
    {
        return None;
    }
    let request = holder
        .take_request()?
        .request(&a.ticket.owner.ctx.client, &a.ticket.owner.ctx.session);
    let a = s
        .app_home
        .about
        .management
        .exports
        .iter_mut()
        .find(|a| a.id == id)?;
    a.holder = Some(holder);
    a.request = Some(request.clone());
    a.phase = Phase::Sent;
    Some(request)
}
pub(super) fn stop(s: &mut ShellState, id: u64) {
    if let Some(a) = s
        .app_home
        .about
        .management
        .exports
        .iter_mut()
        .find(|a| a.id == id && a.phase == Phase::Preparing)
    {
        a.phase = Phase::NotSent;
    }
}
pub(super) fn settle(s: &mut ShellState, id: u64, result: Result<Value, Error>) {
    let Some(a) = s
        .app_home
        .about
        .management
        .exports
        .iter_mut()
        .find(|a| a.id == id && a.phase == Phase::Sent && a.receipt.is_none())
    else {
        return;
    };
    a.phase = match &result {
        Ok(raw) if raw["ok"] == true => {
            a.initiated = a
                .holder
                .as_ref()
                .and_then(|h| h.acknowledge(raw.clone()).ok());
            if a.initiated.is_some() {
                Phase::Initiated
            } else {
                Phase::Unknown
            }
        }
        Ok(raw) if raw["ok"] == false => Phase::Rejected,
        Err(Error::Offline(_)) => Phase::NotSent,
        Err(Error::Api(reason)) if reason.starts_with("workflow_management_not_sent_") => {
            Phase::NotSent
        }
        Err(Error::RateLimited { .. }) => Phase::Rejected,
        _ => Phase::Unknown,
    };
    a.receipt = Some(result);
}
