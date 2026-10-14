//! Main-thread Dock attention, admitted only by an eligible, deduplicated message.
//! Focus state is read off-thread for each candidate; unknown state is quiet.

use std::hash::{Hash, Hasher};
use std::{cell::RefCell, sync::Arc};

use dioxus::prelude::*;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSRequestUserAttentionType};
use super_platinum_core::{
    CoreAppState,
    slack::{events::RtEvent, models::Message},
    state::{DockAttention, cmp_ts, now_secs},
};

use crate::state::ShellState;

pub(crate) mod check;
mod focus;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Owner {
    account: String,
    epoch: u64,
    team: String,
    user: String,
    transport: usize,
    credential: u64,
}

impl Owner {
    fn capture(core: &CoreAppState, team: &str) -> Option<Self> {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        core.session
            .as_ref()
            .map(|session| &session.d_cookie)
            .hash(&mut hash);
        core.session
            .as_ref()
            .and_then(|session| session.workspaces.get(team))
            .map(|workspace| {
                (
                    &workspace.team_id,
                    &workspace.user_id,
                    &workspace.token,
                    &workspace.url,
                )
            })
            .hash(&mut hash);
        Some(Self {
            account: core.active_account.clone()?,
            epoch: core.account_epoch,
            team: team.into(),
            user: core.workspaces.get(team)?.self_user_id.clone(),
            transport: Arc::as_ptr(core.transport.as_ref()?) as usize,
            credential: hash.finish(),
        })
    }

    fn current(&self, core: &CoreAppState, mode: DockAttention) -> bool {
        Self::capture(core, &self.team).as_ref() == Some(self)
            && core
                .workspaces
                .get(&self.team)
                .is_some_and(|workspace| workspace.notifications.dock_attention() == Some(mode))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Ticket {
    generation: u64,
    owner: Owner,
    mode: DockAttention,
}

struct Request {
    ticket: Ticket,
    cancel: Option<Box<dyn FnOnce()>>,
}

impl Drop for Request {
    fn drop(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel();
        }
    }
}

#[derive(Default)]
struct Controller {
    generation: u64,
    pending: Option<Ticket>,
    candidate: Option<(u64, Message)>,
    active: Option<Request>,
}

enum Admission {
    Available,
    CandidateRetired,
    GloballyQuiet,
}

impl Controller {
    fn queue(
        &mut self,
        owner: Owner,
        mode: DockAttention,
        generation: u64,
        message: &Message,
    ) -> Option<Ticket> {
        let ticket = self.begin(owner, mode);
        // Retain the latest eligible candidate while sharing one Focus read.
        self.candidate = Some((generation, message.clone()));
        ticket
    }

    fn begin(&mut self, owner: Owner, mode: DockAttention) -> Option<Ticket> {
        // A burst needs one Focus read, not one blocking task per message.
        if self
            .pending
            .as_ref()
            .is_some_and(|ticket| ticket.owner == owner && ticket.mode == mode)
        {
            return None;
        }
        self.generation = self.generation.wrapping_add(1);
        let ticket = Ticket {
            generation: self.generation,
            owner,
            mode,
        };
        self.pending = Some(ticket.clone());
        Some(ticket)
    }

    fn cancel(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.pending = None;
        self.candidate = None;
        self.active = None;
    }

    fn complete(
        &mut self,
        ticket: Ticket,
        admission: Admission,
        request: impl FnOnce(DockAttention) -> Option<Box<dyn FnOnce()>>,
    ) {
        if self.pending.as_ref() != Some(&ticket) {
            return;
        }
        self.pending = None;
        self.candidate = None;
        match admission {
            Admission::GloballyQuiet => {
                self.active = None;
                return;
            }
            Admission::CandidateRetired => return,
            Admission::Available => {}
        }
        if self.active.as_ref().is_some_and(|active| {
            active.ticket.owner == ticket.owner
                && active.ticket.mode == ticket.mode
                && ticket.mode == DockAttention::UntilActive
        }) {
            return;
        }
        // Retire the exact prior AppKit ID before starting its replacement.
        self.active = None;
        if let Some(cancel) = request(ticket.mode) {
            self.active = Some(Request {
                ticket,
                cancel: Some(cancel),
            });
        }
    }

    fn maintain(&mut self, active: bool, current: impl Fn(&Ticket) -> bool) {
        if active
            || self.pending.as_ref().is_some_and(|ticket| !current(ticket))
            || self
                .active
                .as_ref()
                .is_some_and(|request| !current(&request.ticket))
        {
            self.cancel();
        }
    }
}

thread_local! {
    // NSApplication and its request ID never leave the Dioxus/AppKit thread.
    static ATTENTION: RefCell<Controller> = RefCell::new(Controller::default());
}

fn app_active() -> bool {
    MainThreadMarker::new().is_none_or(|mtm| NSApplication::sharedApplication(mtm).isActive())
}

fn native_request(mode: DockAttention) -> Option<Box<dyn FnOnce()>> {
    let app = NSApplication::sharedApplication(MainThreadMarker::new()?);
    if app.isActive() {
        return None;
    }
    let kind = match mode {
        DockAttention::Once => NSRequestUserAttentionType::InformationalRequest,
        DockAttention::UntilActive => NSRequestUserAttentionType::CriticalRequest,
    };
    let id = app.requestUserAttention(kind);
    (id >= 0).then(|| Box::new(move || app.cancelUserAttentionRequest(id)) as Box<dyn FnOnce()>)
}

fn read_focus() -> focus::Status {
    use objc2_foundation::{NSBundle, NSCalendar, NSCalendarUnit, NSDate};
    let Some(home) = std::env::var_os("HOME") else {
        return focus::Status::Unknown;
    };
    objc2::rc::autoreleasepool(|_| {
        let bundle = NSBundle::mainBundle()
            .bundleIdentifier()
            .map(|value| value.to_string());
        let time = NSCalendar::currentCalendar().components_fromDate(
            NSCalendarUnit::Hour | NSCalendarUnit::Minute,
            &NSDate::date(),
        );
        let minutes = time.hour() * 60 + time.minute();
        if !(0..1440).contains(&minutes) {
            return focus::Status::Unknown;
        }
        focus::read(
            std::path::Path::new(&home),
            bundle.as_deref(),
            minutes as u16,
        )
    })
}

fn message_event(event: &RtEvent) -> Option<&Message> {
    match event {
        RtEvent::Message(message) | RtEvent::DesktopNotification(message) => Some(message),
        _ => None,
    }
}

fn still_eligible(
    core: &CoreAppState,
    ticket: &Ticket,
    generation: u64,
    message: &Message,
) -> bool {
    if !ticket.owner.current(core, ticket.mode) {
        return false;
    }
    let Some(workspace) = core.workspaces.get(&ticket.owner.team) else {
        return false;
    };
    if workspace.rt_generation != generation
        || workspace
            .notifications
            .message_options(workspace, message, now_secs())
            .is_none()
    {
        return false;
    }
    let (Some(channel), Some(ts)) = (message.channel.as_ref(), message.ts.as_ref()) else {
        return false;
    };
    let thread = message.thread_ts.as_ref().filter(|root| *root != ts);
    let bag = thread
        .and_then(|root| {
            core.threads
                .get(&(ticket.owner.team.clone(), channel.clone(), root.clone()))
        })
        .or_else(|| workspace.messages.get(channel));
    !bag.is_some_and(|bag| cmp_ts(Some(ts), bag.last_read.as_deref()).is_le())
        && (thread.is_some()
            || !workspace
                .channels
                .get(channel)
                .is_some_and(|channel| cmp_ts(Some(ts), channel.last_read.as_deref()).is_le()))
}

pub(crate) fn request(
    state: Signal<ShellState>,
    core: &CoreAppState,
    team: &str,
    generation: u64,
    event: &RtEvent,
) {
    if crate::fixture::is_fixture() || app_active() {
        return;
    }
    // The caller already performed eligibility and deduplication; keep this
    // boundary explicit because huddle invites share the notification tracker.
    let Some(message) = message_event(event) else {
        return;
    };
    let ticket = {
        let Some(owner) = Owner::capture(core, team) else {
            return;
        };
        let Some(mode) = core
            .workspaces
            .get(team)
            .and_then(|w| w.notifications.dock_attention())
        else {
            return;
        };
        let Some(ticket) = ATTENTION
            .with_borrow_mut(|controller| controller.queue(owner, mode, generation, message))
        else {
            return;
        };
        ticket
    };
    spawn(async move {
        let status = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            tokio::task::spawn_blocking(read_focus),
        )
        .await;
        let globally_available = matches!(status, Ok(Ok(focus::Status::Available)))
            && !app_active()
            && ticket.owner.current(&state.peek().core, ticket.mode);
        ATTENTION.with_borrow_mut(|controller| {
            let admission = if !globally_available {
                Admission::GloballyQuiet
            } else if controller
                .candidate
                .as_ref()
                .is_some_and(|(generation, message)| {
                    still_eligible(&state.peek().core, &ticket, *generation, message)
                })
            {
                Admission::Available
            } else {
                Admission::CandidateRetired
            };
            controller.complete(ticket, admission, native_request);
        });
    });
}

pub(crate) fn cancel() {
    ATTENTION.with_borrow_mut(Controller::cancel);
}

/// Reuses the existing shell tick, and does no work unless a request is owned.
pub(crate) fn maintain(core: &CoreAppState) {
    ATTENTION.with_borrow_mut(|controller| {
        if controller.active.is_some() || controller.pending.is_some() {
            controller.maintain(app_active(), |ticket| {
                ticket.owner.current(core, ticket.mode)
            });
        }
    });
}

pub(crate) fn use_cleanup() {
    use_drop(cancel);
    dioxus::desktop::use_wry_event_handler(|event, _| {
        if matches!(
            event,
            dioxus::desktop::tao::event::Event::WindowEvent {
                event: dioxus::desktop::WindowEvent::Focused(true),
                ..
            }
        ) {
            cancel();
        }
    });
}

#[cfg(test)]
mod tests;
