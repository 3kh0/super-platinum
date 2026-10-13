//! Query cancellation owns only debounce/painting. A dispatched read completes
//! independently so its Retry-After cannot disappear with the old query.
use super::*;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use super_platinum_core::slack::Error;
use tokio::sync::{Notify, oneshot};

const MAX_OWNERS: usize = 32;
#[derive(Clone, Debug, PartialEq, Eq)]
struct Account {
    id: Option<String>,
    epoch: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Owner {
    team: String,
    user: String,
}
#[derive(Clone)]
struct Ticket {
    account: Account,
    owner: Owner,
    serial: u64,
}
struct Entry {
    serial: u64,
    in_flight: bool,
    retry_at: Option<Instant>,
}
#[derive(Default)]
struct Admission {
    account: Option<Account>,
    entries: HashMap<Owner, Entry>,
    serial: u64,
}
enum Decision {
    Start(Ticket),
    Busy,
    Cooling,
    Full,
}
impl Admission {
    fn begin(&mut self, account: Account, owner: Owner, now: Instant) -> Decision {
        if self.account.as_ref() != Some(&account) {
            self.entries.clear();
            self.account = Some(account.clone());
        }
        self.entries
            .retain(|_, e| e.in_flight || e.retry_at.is_some_and(|until| now < until));
        if let Some(entry) = self.entries.get(&owner) {
            return if entry.in_flight {
                Decision::Busy
            } else {
                Decision::Cooling
            };
        }
        if self.entries.len() >= MAX_OWNERS {
            return Decision::Full;
        }
        self.serial = self.serial.wrapping_add(1);
        let ticket = Ticket {
            account,
            owner: owner.clone(),
            serial: self.serial,
        };
        self.entries.insert(
            owner,
            Entry {
                serial: ticket.serial,
                in_flight: true,
                retry_at: None,
            },
        );
        Decision::Start(ticket)
    }
    fn finish(&mut self, ticket: &Ticket, now: Instant, error: Option<&Error>) {
        if self.account.as_ref() != Some(&ticket.account) {
            return;
        }
        let Some(entry) = self
            .entries
            .get_mut(&ticket.owner)
            .filter(|e| e.serial == ticket.serial)
        else {
            return;
        };
        entry.in_flight = false;
        let seconds = match error {
            Some(Error::RateLimited { retry_after_secs }) => {
                Some(retry_after_secs.unwrap_or(5).max(1))
            }
            Some(Error::HttpStatus {
                retry_after_secs: Some(seconds),
                ..
            }) => Some((*seconds).max(1)),
            _ => None,
        };
        entry.retry_at = seconds.map(|seconds| {
            now.checked_add(Duration::from_secs(seconds))
                .unwrap_or_else(|| now + Duration::from_secs(86400))
        });
    }
}
struct Shared {
    admission: RefCell<Admission>,
    changed: Notify,
}
thread_local! {
    // Main-thread Dioxus tasks share this bounded state across channel/thread
    // component mounts. It retains no query, credentials, results or transport.
    static SHARED: Rc<Shared> = Rc::new(Shared {admission: RefCell::new(Admission::default()),changed: Notify::new()});
}
pub(super) const COOLING: &str =
    "Slack asked us to wait before searching again. Cached matches are shown.";
pub(super) fn owner_current(shell: &ShellState, ctx: &Context) -> bool {
    ctx.current(shell) && credentials_current(shell, ctx)
}
fn account_current(shell: &ShellState, account: &Account) -> bool {
    shell.core.active_account == account.id && shell.core.account_epoch == account.epoch
}
fn credentials_current(shell: &ShellState, ctx: &Context) -> bool {
    shell.core.account_epoch == ctx.epoch
        && shell
            .core
            .transport
            .as_ref()
            .is_some_and(|t| std::sync::Arc::ptr_eq(t, &ctx.transport))
        && shell
            .core
            .session
            .as_ref()
            .and_then(|s| s.workspaces.get(&ctx.session.team_id))
            .is_some_and(|s| s.user_id == ctx.session.user_id && s.token == ctx.session.token)
}

pub(super) async fn fetch(
    state: Signal<ShellState>,
    ctx: Rc<Context>,
    query: String,
    valid: impl Fn(&ShellState) -> bool,
) -> Result<Vec<User>, &'static str> {
    let shared = SHARED.with(Clone::clone);
    let account = Account {
        id: state.peek().core.active_account.clone(),
        epoch: ctx.epoch,
    };
    let owner = Owner {
        team: ctx.session.team_id.clone(),
        user: ctx.session.user_id.clone(),
    };
    let ticket = loop {
        // Register before checking Busy so completion cannot be lost between
        // checking admission and awaiting the notification.
        let notified = shared.changed.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        if !valid(&state.peek())
            || !account_current(&state.peek(), &account)
            || !owner_current(&state.peek(), &ctx)
        {
            return Err("");
        }
        let decision =
            shared
                .admission
                .borrow_mut()
                .begin(account.clone(), owner.clone(), Instant::now());
        match decision {
            Decision::Start(ticket) => break ticket,
            Decision::Busy => notified.await,
            Decision::Cooling => return Err(COOLING),
            Decision::Full => return Err("Directory searches are busy. Cached matches are shown."),
        }
    };
    let (sender, receiver) = oneshot::channel();
    dioxus::core::spawn_forever(async move {
        // Admission has no await before this final owner check and dispatch.
        let may_send = !sender.is_closed()
            && account_current(&state.peek(), &account)
            && owner_current(&state.peek(), &ctx);
        settle(
            shared,
            ticket,
            sender,
            async {
                if may_send {
                    request(&ctx, &query).await
                } else {
                    Err(Error::Api("mention_owner_changed".into()))
                }
            },
            || account_current(&state.peek(), &account) && credentials_current(&state.peek(), &ctx),
        )
        .await;
    });
    match receiver.await {
        Ok(Ok(users)) => Ok(users),
        Ok(Err(
            Error::RateLimited { .. }
            | Error::HttpStatus {
                retry_after_secs: Some(_),
                ..
            },
        )) => Err(COOLING),
        _ => Err("Directory unavailable. Edit your search to retry."),
    }
}
// The response lifetime is independent of the query receiver. In particular,
// dropping a canceled query must not discard an already-dispatched 429.
async fn settle(
    shared: Rc<Shared>,
    ticket: Ticket,
    sender: oneshot::Sender<Result<Vec<User>, Error>>,
    read: impl std::future::Future<Output = Result<Vec<User>, Error>>,
    owned: impl FnOnce() -> bool,
) {
    let result = read.await;
    shared.admission.borrow_mut().finish(
        &ticket,
        Instant::now(),
        if owned() { result.as_ref().err() } else { None },
    );
    shared.changed.notify_waiters();
    let _ = sender.send(result);
}

async fn request(ctx: &Context, query: &str) -> Result<Vec<User>, Error> {
    let request =
        super_platinum_core::slack::edge::users_search(&ctx.client, &ctx.session, query, 30)
            .map_err(|_| Error::Api("mention_search_unavailable".into()))?;
    let value = ctx.transport.execute_mention_search(request).await?;
    serde_json::from_value(
        value
            .get("results")
            .cloned()
            .ok_or_else(|| Error::Api("mention_search_invalid".into()))?,
    )
    .map_err(|_| Error::Api("mention_search_invalid".into()))
}

#[cfg(test)]
mod tests;
