//! Explicit no-MPIM conversation reads. Native quota policy is separate from SDK ranking.
use super::*;
use read_scope::ReadScope;
use std::{cell::Cell, future::Future, rc::Rc};
use super_platinum_core::slack::{PreparedRequest, models::Channel};

pub(crate) type Rows = (Vec<User>, Vec<Channel>);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Provider {
    Users,
    Channels,
}
impl Provider {
    fn bit(self) -> u8 {
        match self {
            Self::Users => 1,
            Self::Channels => 2,
        }
    }
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Users => "users",
            Self::Channels => "channels",
        }
    }
}

struct InitialState {
    expected: u8,
    arrived: Cell<u8>,
    finished: Cell<u8>,
    notify: tokio::sync::Notify,
}
#[derive(Clone)]
pub(crate) struct InitialGate(Rc<InitialState>);
impl InitialGate {
    fn new(expected: u8) -> Self {
        Self(Rc::new(InitialState {
            expected,
            arrived: Cell::new(0),
            finished: Cell::new(0),
            notify: tokio::sync::Notify::new(),
        }))
    }
    fn arrive(&self, provider: Provider) {
        self.0.arrived.set(self.0.arrived.get() | provider.bit());
        self.0.notify.notify_waiters();
    }
    /// A bundled fixture cannot settle before all enabled initial executions begin.
    pub(crate) async fn wait(&self) -> bool {
        loop {
            let notified = self.0.notify.notified();
            if self.0.arrived.get() & self.0.expected == self.0.expected {
                return true;
            }
            if self.0.finished.get() & !self.0.arrived.get() & self.0.expected != 0 {
                return false;
            }
            notified.await;
        }
    }
}
struct Participant(InitialGate, Provider);
impl Drop for Participant {
    fn drop(&mut self) {
        self.0
            .0
            .finished
            .set(self.0.0.finished.get() | self.1.bit());
        self.0.0.notify.notify_waiters();
    }
}

pub(crate) async fn read_with<F, Fut>(
    scope: &ReadScope,
    context: &Context,
    query: &str,
    filter: &super::super::conversations::Filter,
    execute: F,
) -> Result<Option<Rows>, Error>
where
    F: Fn(Provider, PreparedRequest, ReadScope, Option<InitialGate>) -> Fut,
    Fut: Future<Output = Result<Option<Value>, Error>>,
{
    if !conversation_mask_matches(filter) {
        return Err(Error::Api("conversation_search_unavailable".into()));
    }
    let users_enabled = filter.allows("im");
    let channels_enabled = filter.allows("public") || filter.allows("private");
    let gate = InitialGate::new(u8::from(users_enabled) | (u8::from(channels_enabled) << 1));
    let users = async {
        if !users_enabled {
            return Ok(Some(Vec::new()));
        }
        let _participant = Participant(gate.clone(), Provider::Users);
        let first = Cell::new(true);
        scope
            .lookup(
                |marker| prepare_conversation_page(context, query, filter, true, marker),
                |request| {
                    let initial = first.replace(false).then(|| gate.clone());
                    if initial.is_some() {
                        gate.arrive(Provider::Users);
                    }
                    execute(Provider::Users, request, scope.clone(), initial)
                },
            )
            .await
    };
    let channels = async {
        if !channels_enabled {
            return Ok(Some(Vec::new()));
        }
        let _participant = Participant(gate.clone(), Provider::Channels);
        let first = Cell::new(true);
        scope
            .lookup_channels(
                |marker| prepare_conversation_page(context, query, filter, false, marker),
                |request| {
                    let initial = first.replace(false).then(|| gate.clone());
                    if initial.is_some() {
                        gate.arrive(Provider::Channels);
                    }
                    execute(Provider::Channels, request, scope.clone(), initial)
                },
            )
            .await
    };
    // Settle already-dispatched attempts even after a sibling cools admission.
    let (users, channels) = tokio::join!(users, channels);
    if !scope.current() {
        return Ok(None);
    }
    combine(users, channels)
}

fn combine(
    users: Result<Option<Vec<User>>, Error>,
    channels: Result<Option<Vec<Channel>>, Error>,
) -> Result<Option<Rows>, Error> {
    let longest = [users.as_ref().err(), channels.as_ref().err()]
        .into_iter()
        .flatten()
        .filter_map(|e| {
            if let Error::RateLimited {
                retry_after_secs: Some(seconds),
            } = e
            {
                Some(*seconds)
            } else {
                None
            }
        })
        .max();
    if let Some(seconds) = longest {
        return Err(Error::RateLimited {
            retry_after_secs: Some(seconds),
        });
    }
    // A refused sibling cannot erase this batch's known429 above. Otherwise wait
    // state already belongs to admission; never invent ordinary error fallback.
    if matches!(&users, Ok(None)) || matches!(&channels, Ok(None)) {
        return Ok(None);
    }
    // Keep the legacy aggregate's ordinary Channel-before-Member error priority.
    let Some(channels) = channels? else {
        return Ok(None);
    };
    let Some(users) = users? else { return Ok(None) };
    Ok(Some((users, channels)))
}

#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
mod tests;
