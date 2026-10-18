//! Pure Salesforce connection controller. Parent owns owner checks, once transport,
//! native lease, browser launch and realtime envelope; this module performs no I/O.
use serde_json::Value;
use super_platinum_core::block_actions::home::workflow::salesforce::{
    Action, AuthOutcome, ListReason, Org, Target,
};
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Ticket(u64);
#[derive(Clone, PartialEq)]
pub(super) enum Effect {
    Request { ticket: Ticket, action: Action },
    OpenExternal { ticket: Ticket, url: String },
    None,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pending {
    Request(Ticket, Action),
    Browser(Ticket),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Status {
    Initial,
    Loading,
    Ready,
    MissingOrganization,
    WaitingForBrowser,
    ManualInProgress,
    Failed,
    Cancelled,
}

pub(super) struct Connection {
    target: Target,
    serial: u64,
    pending: Option<Pending>,
    org: Option<Org>,
    status: Status,
    uncertain: bool,
}
impl Connection {
    pub(super) fn new(target: Target) -> Self {
        Self {
            target,
            serial: 0,
            pending: None,
            org: None,
            status: Status::Initial,
            uncertain: false,
        }
    }
    pub(super) fn target(&self) -> &Target {
        &self.target
    }
    pub(super) fn org(&self) -> Option<&Org> {
        self.org.as_ref()
    }
    pub(super) fn status(&self) -> Status {
        self.status
    }
    pub(super) fn busy(&self) -> bool {
        self.pending.is_some()
    }
    pub(super) fn uncertain(&self) -> bool {
        self.uncertain
    }
    pub(super) fn expects(&self, ticket: Ticket, action: Action) -> bool {
        self.pending == Some(Pending::Request(ticket, action))
    }
    /// A caller-proven pre-dispatch refusal or HTTP rate limit does not make
    /// authentication uncertain. It also never repeats the cancelled ticket.
    pub(super) fn not_sent(&mut self, ticket: Ticket) -> bool {
        if self.consume(ticket).is_none() {
            return false;
        }
        self.status = Status::Failed;
        true
    }
    fn available(&self) -> bool {
        !self.busy() && self.status != Status::Cancelled
    }
    fn ticket(&mut self) -> Ticket {
        self.serial = self
            .serial
            .checked_add(1)
            .expect("Salesforce ticket overflow");
        Ticket(self.serial)
    }
    fn request(&mut self, action: Action) -> Effect {
        let ticket = self.ticket();
        self.pending = Some(Pending::Request(ticket, action));
        self.status = Status::Loading;
        Effect::Request { ticket, action }
    }
    pub(super) fn begin(&mut self) -> Option<Effect> {
        (self.status == Status::Initial && !self.busy())
            .then(|| self.request(Action::List(ListReason::Initial)))
    }
    /// Explicit read-only retry. Never repeats an uncertain authentication/disconnect.
    pub(super) fn check(&mut self) -> Option<Effect> {
        self.available()
            .then(|| self.request(Action::List(ListReason::ManualRecheck)))
    }
    pub(super) fn sign_in(&mut self) -> Option<Effect> {
        if !self.available() || self.uncertain || self.org.as_ref().is_none_or(Org::connected) {
            return None;
        }
        Some(self.request(if self.target.jwt() {
            Action::LoginUrl
        } else {
            Action::Manual
        }))
    }
    pub(super) fn disconnect(&mut self) -> Option<Effect> {
        if !self.available() || self.uncertain || !self.org.as_ref().is_some_and(Org::connected) {
            return None;
        }
        Some(self.request(Action::Disconnect))
    }
    fn consume(&mut self, ticket: Ticket) -> Option<Action> {
        let Some(Pending::Request(current, action)) = self.pending else {
            return None;
        };
        if current != ticket {
            return None;
        }
        self.pending = None;
        Some(action)
    }
    pub(super) fn response(&mut self, ticket: Ticket, raw: Value) -> Result<Effect, &'static str> {
        let action = self
            .consume(ticket)
            .ok_or("Salesforce response owner changed")?;
        match self.apply(action, raw) {
            Ok(effect) => Ok(effect),
            Err(error) => {
                self.status = Status::Failed;
                self.uncertain |= !matches!(action, Action::List(_));
                Err(error)
            }
        }
    }
    fn apply(&mut self, action: Action, raw: Value) -> Result<Effect, &'static str> {
        match action {
            Action::List(reason) => {
                let snapshot = self
                    .target
                    .organizations(raw)
                    .map_err(|_| "Salesforce organizations could not be read")?;
                self.org = snapshot.target;
                let Some(org) = &self.org else {
                    self.status = Status::MissingOrganization;
                    return Ok(Effect::None);
                };
                if reason == ListReason::Initial && org.silent_candidate() {
                    return Ok(self.request(Action::Silent));
                }
                self.status = if reason == ListReason::ManualRecheck && !org.connected() {
                    Status::ManualInProgress
                } else {
                    Status::Ready
                };
                // A successful actual read proves current connectivity, not whether a
                // failed mutating call ran. Keep uncertain custody until parent review.
                Ok(Effect::None)
            }
            Action::Silent => {
                let successful = self
                    .target
                    .silent_succeeded(&raw)
                    .map_err(|_| "Salesforce automatic sign-in could not be checked")?;
                // Modal WgYR refetches after failed forced authentication; we also use
                // a real read after success instead of fabricating the resulting row.
                Ok(self.request(Action::List(if successful {
                    ListReason::ManualRecheck
                } else {
                    ListReason::ForcedRefetch
                })))
            }
            Action::Manual | Action::LoginUrl => {
                let outcome = self
                    .target
                    .manual_response(action, &raw)
                    .map_err(|_| "Salesforce sign-in could not be checked")?;
                Ok(match outcome {
                    AuthOutcome::CheckOrganization => {
                        self.request(Action::List(ListReason::ManualRecheck))
                    }
                    AuthOutcome::LoginUrlRequired => self.request(Action::LoginUrl),
                    AuthOutcome::OpenExternal(url) => {
                        let ticket = self.ticket();
                        self.pending = Some(Pending::Browser(ticket));
                        self.status = Status::WaitingForBrowser;
                        Effect::OpenExternal { ticket, url }
                    }
                })
            }
            Action::Disconnect => {
                if raw["ok"] != true {
                    return Err("Salesforce sign-out could not be checked");
                }
                Ok(self.request(Action::List(ListReason::ManualRecheck)))
            }
        }
    }
    /// Window focus is merely permission to perform this read, never auth proof.
    pub(super) fn browser_returned(&mut self, ticket: Ticket) -> Option<Effect> {
        if self.pending != Some(Pending::Browser(ticket)) {
            return None;
        }
        self.pending = None;
        Some(self.request(Action::List(ListReason::ManualRecheck)))
    }
    pub(super) fn browser_failed(&mut self, ticket: Ticket) -> bool {
        if self.pending != Some(Pending::Browser(ticket)) {
            return false;
        }
        self.pending = None;
        self.status = Status::Failed;
        true
    }
    pub(super) fn failed(&mut self, ticket: Ticket) -> Option<Effect> {
        let action = self.consume(ticket)?;
        if action == Action::Silent {
            self.uncertain = true;
            Some(self.request(Action::List(ListReason::ForcedRefetch)))
        } else {
            self.status = Status::Failed;
            self.uncertain |= !matches!(action, Action::List(_));
            Some(Effect::None)
        }
    }
    /// Explicit local review of a successful fresh list result. Parent records this
    /// disposition against the failed once-attempt; no mutating request is repeated.
    pub(super) fn use_current_connection(&mut self) -> bool {
        if !self.available()
            || !matches!(
                self.status,
                Status::Ready | Status::ManualInProgress | Status::MissingOrganization
            )
        {
            return false;
        }
        self.uncertain = false;
        true
    }
    pub(super) fn cancel(&mut self) {
        self.pending = None;
        self.org = None;
        self.status = Status::Cancelled;
    }
    /// Parent has reacquired the same native context and an explicit user Add
    /// action. Only a fresh organization read resumes; the unknown write stays
    /// blocked until a separately reviewed current result is acknowledged.
    pub(super) fn review_after_cancel(&mut self) -> Option<Effect> {
        if self.status != Status::Cancelled || !self.uncertain || self.pending.is_some() {
            return None;
        }
        self.status = Status::Failed;
        self.check()
    }
}
