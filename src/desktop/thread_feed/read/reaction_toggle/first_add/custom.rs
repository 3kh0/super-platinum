//! Purpose-owned direct definitions; provisional catalogues cannot mint these.
use super::*;
use super_platinum_core::{
    slack::{
        api::emoji_catalog::Complete,
        realtime::{ReadBarrier, ReadIdentity},
    },
    thread_reaction::first_add_custom::{CustomChoice, CustomDomain, CustomToggle},
};
mod controls;
mod display;
mod read;
pub(crate) use controls::{retry, set_mode};
pub(crate) use display::{ImageDisplay, assets as image_assets};

#[derive(Default)]
pub(super) struct State {
    pub mode: bool,
    serial: u64,
    pub loading: bool,
    pub ready: Option<Arc<Ready>>,
    pub notice: String,
}
struct Receipt {
    retired: AtomicBool,
    open: Open,
    account: Option<String>,
    revision: u64,
    generation: u64,
    identity: ReadIdentity,
    _accepted: ReadBarrier,
    media: crate::media::MediaRegistry,
    _complete: Arc<Complete>,
}
impl Receipt {
    fn data_current(&self, s: &ShellState) -> bool {
        let source = &self.open.scope.source;
        if self.retired.load(Ordering::Acquire) {
            return false;
        }
        let valid = self.account == s.core.active_account
            && self.open.base_current(s)
            && source.client.same_configuration(&s.core.client)
            && s.core
                .session
                .as_ref()
                .and_then(|v| v.workspaces.get(&source.team))
                .is_some_and(|now| crate::emoji_preferences::same_session(&source.session, now))
            && s.core.workspaces.get(&source.team).is_some_and(|w| {
                w.custom_emoji_revision == self.revision && w.rt_generation == self.generation
            })
            && super::super::connection(s, &source.team)
                .is_some_and(|c| self.identity.open() && self.identity.is_for(c))
            && self.media.same_registry(&s.media);
        if !valid {
            self.retired.store(true, Ordering::Release);
        }
        // A temporarily queued irrelevant frame is not permanent retirement.
        valid
    }
    fn current(&self, s: &ShellState) -> bool {
        self.data_current(s) && self.open.scope.caught_up(s)
    }
}
pub(super) struct Ready {
    receipt: Receipt,
    domain: CustomDomain,
    choices: Vec<CustomChoice>,
    model: name_search::Model,
}
impl Ready {
    // Pure query data may publish while an irrelevant peer frame is queued.
    // Every display, activation and wire path still requires current/caught_up.
    pub(super) fn data_current(&self, s: &ShellState) -> bool {
        self.receipt.data_current(s)
    }
    pub(super) fn current(&self, s: &ShellState) -> bool {
        self.receipt.current(s)
    }
    pub(super) fn len(&self) -> usize {
        self.choices.len()
    }
    pub(super) fn matching(&self, literal: &str) -> Vec<usize> {
        if name_search::Model::empty(literal) {
            (0..self.choices.len()).take(250).collect()
        } else {
            self.model.query(literal).unwrap_or_default()
        }
    }
    pub(super) fn selection(self: &Arc<Self>, index: usize) -> Option<Selection> {
        let choice = self.choices.get(index)?.clone();
        let fallback = format!(":{}:", choice.name());
        Some(Selection {
            ready: self.clone(),
            choice,
            fallback,
        })
    }
}
#[derive(Clone)]
pub(super) struct Selection {
    ready: Arc<Ready>,
    choice: CustomChoice,
    fallback: String,
}
impl Selection {
    pub(super) fn name(&self) -> &str {
        self.choice.name()
    }
    pub(super) fn fallback(&self) -> &str {
        &self.fallback
    }
    pub(super) fn current(&self, s: &ShellState) -> bool {
        self.ready.current(s)
    }
    pub(super) fn select(
        &self,
        policy: &RootPolicy,
        membership: &RootMembership,
        user: &str,
    ) -> std::result::Result<Toggle, &'static str> {
        let toggle = membership.first_add_custom_selection(
            policy,
            &self.ready.domain,
            policy.preferences(),
            &self.choice,
            user,
        )?;
        (toggle.name() == self.choice.name())
            .then(|| Toggle {
                ready: self.ready.clone(),
                toggle,
            })
            .ok_or("Direct custom reaction is unavailable.")
    }
}
pub(super) struct Toggle {
    ready: Arc<Ready>,
    toggle: CustomToggle,
}
impl Toggle {
    pub(super) fn name(&self) -> &str {
        self.toggle.name()
    }
    pub(super) fn adding(&self) -> bool {
        self.toggle.adding()
    }
    pub(super) fn current(
        &self,
        policy: &RootPolicy,
        membership: &RootMembership,
        user: &str,
    ) -> bool {
        self.toggle.current(
            policy,
            &self.ready.domain,
            policy.preferences(),
            membership,
            user,
        )
    }
}
pub(super) fn ready_for(session: &Session) -> Option<Arc<Ready>> {
    session
        .custom
        .mode
        .then(|| session.custom.ready.clone())
        .flatten()
}
pub(super) fn image(
    owner: Arc<Picker>,
    ready: Arc<Ready>,
    frame: u64,
    index: usize,
) -> Option<ImageDisplay> {
    ImageDisplay::new(owner, ready, frame, index)
}
fn parent_current(p: &Picker, s: &ShellState, prepared: &Arc<Prepared>) -> bool {
    p.current(s)
        && p.open.scope.caught_up(s)
        && prepared.loaded.current(s)
        && crate::emoji_catalog::current(s).is_some_and(|v| Arc::ptr_eq(&v, &prepared.loaded))
}
fn parent_ready(p: &Picker, s: &ShellState, prepared: &Arc<Prepared>) -> bool {
    parent_current(p, s, prepared)
        && !crate::reaction_writes::registry(s).blocked(&p.open.scope.physical(&p.open.key))
}
