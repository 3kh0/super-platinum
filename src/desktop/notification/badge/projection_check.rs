//! Shared disconnected fixture mutations. Platform checks observe production hooks.
use crate::state::ShellState;
use dioxus::prelude::*;
use serde_json::json;

#[derive(Clone, Copy, Debug)]
pub(super) enum Step {
    Activity,
    Zero,
    Later,
    HiddenLater,
    RestoreLater,
    RetireAccount,
}
pub(super) const STEPS: [Step; 6] = [
    Step::Activity,
    Step::Zero,
    Step::Later,
    Step::HiddenLater,
    Step::RestoreLater,
    Step::RetireAccount,
];

impl Step {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Activity => "activityExcludesReaction",
            Self::Zero => "activityZero",
            Self::Later => "eligibleLater",
            Self::HiddenLater => "hiddenLater",
            Self::RestoreLater => "positiveBeforeRetirement",
            Self::RetireAccount => "accountRetirement",
        }
    }
    pub(super) fn expected(self) -> Option<&'static str> {
        match self {
            Self::Activity => Some("6"),
            Self::Later | Self::RestoreLater => Some("4"),
            _ => None,
        }
    }
    pub(super) fn apply(self, mut state: Signal<ShellState>) -> Result<(), String> {
        require_isolated(state)?;
        let mut shell = state.write();
        match self {
            Self::Activity => {
                shell.signed_in = true;
                shell.core.active_account = Some("dock-projection-fixture".into());
                shell.core.account_epoch = shell.core.account_epoch.wrapping_add(1);
                shell.core.workspaces.retain(|team, _| team == "T1");
                let w = shell
                    .core
                    .workspaces
                    .get_mut("T1")
                    .ok_or("Missing synthetic workspace")?;
                w.notifications = Default::default();
                w.notifications.dock_loaded = true;
                let rules = super_platinum_core::slack::api::parse_badge_experiments(
                    &json!({"ok":true,"assignments":{"activity_inbox":{"group":"on"}}}),
                )
                .map_err(|_| "Synthetic badge policy rejected")?;
                let ticket = w.activity_counts.begin_experiments();
                w.activity_counts.apply_experiments(ticket, rules);
                w.apply_activity_counts(
                    serde_json::from_value(json!({"at_user":6,"message_reaction":900})).unwrap(),
                );
                shell.refresh_dock_badge();
            }
            Self::Zero => {
                shell
                    .core
                    .workspaces
                    .get_mut("T1")
                    .ok_or("Missing synthetic workspace")?
                    .apply_activity_counts(serde_json::from_value(json!({})).unwrap());
                shell.refresh_dock_badge_for("T1");
            }
            Self::Later => {
                let w = shell
                    .core
                    .workspaces
                    .get_mut("T1")
                    .ok_or("Missing synthetic workspace")?;
                w.notifications
                    .global
                    .insert("global_reminders_in_activity".into(), json!(false));
                w.dock_counts.apply_overdue_count(4);
                shell.refresh_dock_badge_for("T1");
            }
            Self::HiddenLater | Self::RestoreLater => {
                let shown = matches!(self, Self::RestoreLater);
                shell
                    .core
                    .workspaces
                    .get_mut("T1")
                    .ok_or("Missing synthetic workspace")?
                    .notifications
                    .user
                    .insert(
                        "tab_rail_customization".into(),
                        json!(json!({"later":shown}).to_string()),
                    );
                shell.refresh_dock_badge_for("T1");
            }
            Self::RetireAccount => {
                shell.core.active_account = None;
                shell.core.account_epoch = shell.core.account_epoch.wrapping_add(1);
                // Deliberately no refresh: the cached getter and native owner
                // retirement must clear before a replacement loader completes.
            }
        }
        Ok(())
    }
}

pub(super) fn require_isolated(state: Signal<ShellState>) -> Result<(), String> {
    let shell = state.peek();
    if !crate::fixture::is_fixture()
        || shell.core.session.is_some()
        || shell.core.transport.is_some()
    {
        Err("Badge projection check requires a disconnected fixture".into())
    } else {
        Ok(())
    }
}
