//! Change-triggered native badge sink. Projection and input readiness live in core.
use dioxus::desktop::DesktopContext;
use super_platinum_core::state::DockBadge;

pub(crate) mod check;
#[cfg(test)]
mod tests;

/// The expensive workspace projection runs at count mutation boundaries. This
/// subscription reads only its cached scalar and account identity, so media
/// arrivals and typing renders never scan conversations or write to AppKit.
pub(crate) fn use_badge(
    state: dioxus::prelude::Signal<crate::state::ShellState>,
    desktop: DesktopContext,
) {
    use dioxus::prelude::{ReadableExt, use_effect, use_future, use_hook, use_memo};
    use std::{cell::RefCell, rc::Rc};
    let controller = use_hook(|| Rc::new(RefCell::new(Controller::new(desktop.clone()))));
    let value = use_memo(move || {
        let shell = state.read();
        let owner = shell
            .core
            .active_account
            .as_ref()
            .filter(|_| shell.signed_in)
            .map(|account| Owner {
                account: account.clone(),
                epoch: shell.core.account_epoch,
            });
        (owner, shell.dock_badge_value())
    });
    use_effect(move || {
        let (owner, badge) = value();
        if !check::requested() {
            controller.borrow_mut().update(owner, badge);
        }
    });
    use_future(move || check::run(state, desktop.clone()));
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Owner {
    pub account: String,
    pub epoch: u64,
}

/// Kept in a root-scoped hook. This owns no timers, shell subscriptions or scans.
/// The !Send callback keeps Tao's AppKit setter on the Dioxus main thread.
pub(crate) struct Controller {
    owner: Option<Owner>,
    applied: Option<Option<String>>,
    write: Box<dyn FnMut(Option<String>)>,
}

impl Controller {
    pub(crate) fn new(desktop: DesktopContext) -> Self {
        use dioxus::desktop::tao::platform::macos::WindowExtMacOS;
        Self::with_writer(move |label| desktop.window.set_badge_label(label))
    }

    fn with_writer(write: impl FnMut(Option<String>) + 'static) -> Self {
        Self {
            owner: None,
            applied: None,
            write: Box::new(write),
        }
    }

    pub(crate) fn update(&mut self, owner: Option<Owner>, badge: DockBadge) {
        if self.owner != owner {
            // An account/epoch transition must retire the previous owner's
            // label, even when the new owner has the same numeric total.
            self.apply(None);
            self.owner = owner;
        }
        let label = self.owner.as_ref().and_then(|_| label(badge));
        self.apply(label);
    }

    fn apply(&mut self, label: Option<String>) {
        if self.applied.as_ref() == Some(&label) {
            return;
        }
        (self.write)(label.clone());
        self.applied = Some(label);
    }
}
impl Drop for Controller {
    fn drop(&mut self) {
        if self.applied.as_ref().is_some_and(|label| label.is_some()) {
            (self.write)(None);
        }
    }
}

fn label(badge: DockBadge) -> Option<String> {
    match badge {
        DockBadge::Empty | DockBadge::Count(0) => None,
        DockBadge::Dot => Some("•".into()),
        DockBadge::Count(count) => Some(count.to_string()),
    }
}
