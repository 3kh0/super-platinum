//! Accepted full-boot mode, independent of posting authority and socket count.
//! Missing fields in an observed boot are falsey; an unobserved boot is unknown.
//! Native has no Slack renderer URL/localStorage group override: the separate
//! default noGroup policy does not come from an invented server experiment.
use crate::directory_member_fetches::{Context, Key, unavailable};
use crate::state::ShellState;
use serde_json::Value;
use std::collections::BTreeMap;
use super_platinum_core::slack::Error;

#[derive(Default)]
pub(super) struct Registry {
    observations: BTreeMap<Key, Observation>,
}
struct Observation {
    // Contains credentials; intentionally neither Debug nor Serialize.
    context: Context,
    subscription: bool,
}
impl Registry {
    pub(super) fn retain(&mut self, shell: &ShellState) {
        self.observations
            .retain(|_, value| value.context.account_current(shell));
    }
}

/// Project before BootRead consumes the raw body, publish only after its owned
/// full-boot apply returns true. Failed/stale reads must never call accepted.
pub(crate) struct Boot {
    subscription: bool,
}
impl Boot {
    pub(crate) fn capture(raw: &Value) -> Self {
        Self {
            subscription: raw.is_object()
                && !truthy(raw.get("no_login"))
                && !truthy(raw.get("noSocket")),
        }
    }
    pub(crate) fn accepted(self, shell: &mut ShellState, team: &str) -> Result<(), Error> {
        let context = Context::capture(shell, team)?;
        shell
            .directory_member_fetches
            .automatic
            .mode
            .observations
            .insert(
                context.key(),
                Observation {
                    context,
                    subscription: self.subscription,
                },
            );
        super::notify(shell, team);
        Ok(())
    }
}

/// Recheck during asynchronous publication as well as admission. A newer boot
/// can disable subscription mode without replacing the account/session.
pub(crate) fn observed(shell: &ShellState, team: &str) -> Result<(), Error> {
    let context = Context::capture(shell, team)?;
    let observation = shell
        .directory_member_fetches
        .automatic
        .mode
        .observations
        .get(&context.key())
        .ok_or_else(unavailable)?;
    if observation.subscription && observation.context.account_current(shell) {
        Ok(())
    } else {
        Err(unavailable())
    }
}

fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null | Value::Bool(false)) => false,
        Some(Value::Number(number)) => number.as_f64() != Some(0.0),
        Some(Value::String(text)) => !text.is_empty(),
        Some(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;
    use super_platinum_core::{
        config::{Session, WorkspaceSession},
        slack::Transport,
        state::Workspace,
    };

    fn shell() -> ShellState {
        let mut shell = ShellState::login(crate::media::MediaRegistry::default());
        let workspace = WorkspaceSession {
            team_id: "T1".into(),
            enterprise_id: None,
            user_id: "U00000009".into(),
            name: "Synthetic".into(),
            url: "https://example.invalid".into(),
            token: "synthetic-token".into(),
        };
        shell.core.active_account = Some("A".into());
        shell.core.active_team = Some("T1".into());
        shell.core.transport = Some(Arc::new(Transport::new("synthetic-cookie").unwrap()));
        shell.core.workspaces.clear();
        shell
            .core
            .workspaces
            .insert("T1".into(), Workspace::from_session(&workspace));
        shell.core.session = Some(Session {
            d_cookie: "synthetic-cookie".into(),
            unified_user_client_enabled: BTreeMap::new(),
            workspaces: BTreeMap::from([("T1".into(), workspace)]),
        });
        shell
    }

    #[test]
    fn unobserved_is_not_observed_missing_flags() {
        let mut shell = shell();
        assert!(observed(&shell, "T1").is_err());
        let captured = Boot::capture(&json!({}));
        assert!(observed(&shell, "T1").is_err());
        captured.accepted(&mut shell, "T1").unwrap();
        assert!(observed(&shell, "T1").is_ok());
        assert!(observed(&shell, "T2").is_err());
    }

    #[test]
    fn flags_follow_source_truthiness_not_bool_coercion_or_native_connectivity() {
        for key in ["no_login", "noSocket"] {
            for (value, allowed) in [
                (Value::Null, true),
                (json!(false), true),
                (json!(0), true),
                (json!(""), true),
                (json!(true), false),
                (json!(1), false),
                (json!("false"), false),
                (json!([]), false),
                (json!({}), false),
            ] {
                let mut shell = shell();
                let raw = Value::Object(serde_json::Map::from_iter([(key.into(), value)]));
                Boot::capture(&raw).accepted(&mut shell, "T1").unwrap();
                assert_eq!(observed(&shell, "T1").is_ok(), allowed, "{raw}");
            }
        }
    }

    #[test]
    fn new_boot_can_revoke_same_owner_but_socket_generation_does_not_erase_boot() {
        let mut shell = shell();
        Boot::capture(&json!({}))
            .accepted(&mut shell, "T1")
            .unwrap();
        shell.core.workspaces.get_mut("T1").unwrap().rt_generation += 1;
        assert!(observed(&shell, "T1").is_ok());
        Boot::capture(&json!({"noSocket":true}))
            .accepted(&mut shell, "T1")
            .unwrap();
        assert!(observed(&shell, "T1").is_err());
        Boot::capture(&json!({"noSocket":false}))
            .accepted(&mut shell, "T1")
            .unwrap();
        assert!(observed(&shell, "T1").is_ok());
    }

    #[test]
    fn complete_owner_replacement_rejects_old_observation() {
        for variant in 0..6 {
            let mut shell = shell();
            Boot::capture(&json!({}))
                .accepted(&mut shell, "T1")
                .unwrap();
            match variant {
                0 => shell.core.active_account = Some("B".into()),
                1 => shell.core.account_epoch += 1,
                2 => shell.core.session.as_mut().unwrap().d_cookie = "changed".into(),
                3 => {
                    shell
                        .core
                        .session
                        .as_mut()
                        .unwrap()
                        .workspaces
                        .get_mut("T1")
                        .unwrap()
                        .token = "changed".into()
                }
                4 => {
                    shell.core.transport =
                        Some(Arc::new(Transport::new("synthetic-cookie").unwrap()))
                }
                _ => shell.core.workspaces.get_mut("T1").unwrap().self_user_id = "UOTHER".into(),
            }
            assert!(observed(&shell, "T1").is_err(), "variant {variant}");
        }
    }
}
