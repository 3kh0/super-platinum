//! Non-secret toast destinations. The OS retains these across app restarts;
//! admission therefore uses stable account/viewer identities, not socket epochs.
use serde::{Deserialize, Serialize};
use super_platinum_core::CoreAppState;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Destination {
    version: u8,
    account: String,
    team: String,
    user: String,
    channel: String,
    message: Option<String>,
    thread: Option<String>,
}

impl Destination {
    pub(super) fn capture(
        core: &CoreAppState,
        team: &str,
        channel: &str,
        message: Option<&str>,
        thread: Option<&str>,
    ) -> Option<Self> {
        let workspace = core.workspaces.get(team)?;
        let result = Self {
            version: 1,
            account: core.active_account.clone()?,
            team: team.into(),
            user: workspace.self_user_id.clone(),
            channel: channel.into(),
            message: message.map(str::to_owned),
            thread: thread
                .filter(|root| Some(*root) != message)
                .map(str::to_owned),
        };
        (result.valid() && result.current(core)).then_some(result)
    }

    fn valid(&self) -> bool {
        self.version == 1
            && !self.account.is_empty()
            && self.account.len() <= 128
            && self
                .account
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-')
            && identifier(&self.team, b"T")
            && identifier(&self.user, b"UW")
            && identifier(&self.channel, b"CGD")
            && self.message.as_deref().is_none_or(timestamp)
            && self.thread.as_deref().is_none_or(timestamp)
    }

    pub(super) fn current(&self, core: &CoreAppState) -> bool {
        core.active_account.as_ref() == Some(&self.account)
            && core.workspaces.get(&self.team).is_some_and(|workspace| {
                workspace.team_id == self.team && workspace.self_user_id == self.user
            })
            && core
                .session
                .as_ref()
                .and_then(|session| session.workspaces.get(&self.team))
                .is_some_and(|session| session.team_id == self.team && session.user_id == self.user)
    }

    pub(super) fn launch(&self) -> String {
        serde_json::to_string(self).expect("notification destination is serializable")
    }

    pub(super) fn link(&self) -> String {
        // Only typed identifiers reach this builder; URL encoding also makes it
        // impossible for a future additional field to inject query parameters.
        let mut url = url::Url::parse("slack://channel").unwrap();
        let mut query = url.query_pairs_mut();
        query
            .append_pair("team", &self.team)
            .append_pair("id", &self.channel);
        if let Some(message) = &self.message {
            query.append_pair("message", message);
        }
        if let Some(thread) = &self.thread {
            query.append_pair("thread_ts", thread);
        }
        drop(query);
        url.to_string()
    }
}

fn identifier(value: &str, prefixes: &[u8]) -> bool {
    value.len() >= 2
        && value.len() <= 128
        && prefixes.contains(&value.as_bytes()[0])
        && value.bytes().all(|c| c.is_ascii_alphanumeric())
}
fn timestamp(value: &str) -> bool {
    let Some((seconds, fraction)) = value.split_once('.') else {
        return false;
    };
    !seconds.is_empty()
        && seconds.len() <= 20
        && seconds.bytes().all(|c| c.is_ascii_digit())
        && !fraction.is_empty()
        && fraction.len() <= 6
        && fraction.bytes().all(|c| c.is_ascii_digit())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Activation {
    Foreground,
    Destination(Destination),
}
impl Activation {
    pub(super) fn parse(input: &str) -> Option<Self> {
        if input == "open" {
            return Some(Self::Foreground);
        }
        if input.len() > 2048 {
            return None;
        }
        let destination: Destination = serde_json::from_str(input).ok()?;
        destination
            .valid()
            .then_some(Self::Destination(destination))
    }
}

/// Only the latest click matters while startup loads the account. Completion
/// drains once even for login/mismatched accounts, never after a later sign-in.
#[derive(Default)]
pub(super) struct Pending {
    ready: bool,
    latest: Option<Activation>,
}
impl Pending {
    pub(super) const fn new() -> Self {
        Self {
            ready: false,
            latest: None,
        }
    }
    pub(super) fn push(&mut self, activation: Activation) {
        self.latest = Some(activation);
    }
    pub(super) fn ready(&mut self) {
        self.ready = true;
    }
    pub(super) fn take(&mut self) -> Option<Activation> {
        if self.ready { self.latest.take() } else { None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source() -> (CoreAppState, Destination) {
        let mut shell = crate::state::ShellState::fixture(crate::media::MediaRegistry::default());
        shell.core.active_account = Some("fixture-account".into());
        shell.core.workspaces.get_mut("T1").unwrap().self_user_id = "USELF".into();
        let user = shell.core.workspaces["T1"].self_user_id.clone();
        shell.core.session = Some(super_platinum_core::config::Session {
            unified_user_client_enabled: Default::default(),
            d_cookie: "synthetic".into(),
            workspaces: [(
                "T1".into(),
                super_platinum_core::config::WorkspaceSession {
                    team_id: "T1".into(),
                    enterprise_id: None,
                    user_id: user,
                    name: "Fixture".into(),
                    url: "https://fixture.slack.com".into(),
                    token: "synthetic".into(),
                },
            )]
            .into(),
        });
        let destination = Destination::capture(
            &shell.core,
            "T1",
            "C1",
            Some("20.000001"),
            Some("19.000000"),
        )
        .unwrap();
        (shell.core, destination)
    }
    #[test]
    fn captures_thread_destination_and_same_user_rotation_remains_owned() {
        let (mut core, destination) = source();
        assert_eq!(destination.thread.as_deref(), Some("19.000000"));
        assert_eq!(
            Activation::parse(&destination.launch()),
            Some(Activation::Destination(destination.clone()))
        );
        assert!(destination.link().contains("thread_ts=19.000000"));
        core.account_epoch += 1;
        core.session
            .as_mut()
            .unwrap()
            .workspaces
            .get_mut("T1")
            .unwrap()
            .token = "rotated".into();
        assert!(destination.current(&core));
        assert!(
            Destination::capture(&core, "T1", "C1", Some("20.000001"), Some("20.000001"))
                .unwrap()
                .thread
                .is_none()
        );
    }
    #[test]
    fn different_account_or_viewer_and_removed_session_never_admit_old_route() {
        for mutation in 0..4 {
            let (mut core, destination) = source();
            match mutation {
                0 => core.active_account = Some("other-account".into()),
                1 => core.workspaces.get_mut("T1").unwrap().self_user_id = "UOTHER".into(),
                2 => {
                    core.session
                        .as_mut()
                        .unwrap()
                        .workspaces
                        .get_mut("T1")
                        .unwrap()
                        .user_id = "UOTHER".into()
                }
                _ => core.session = None,
            }
            assert!(!destination.current(&core));
        }
    }
    #[test]
    fn rejects_unknown_versions_malformed_and_query_injection_but_preserves_preview() {
        let (_, destination) = source();
        assert_eq!(Activation::parse("open"), Some(Activation::Foreground));
        for input in [
            "open extra".into(),
            "https://example.test".into(),
            "x".repeat(2049),
            destination
                .launch()
                .replace("\"version\":1", "\"version\":2"),
            destination.launch().replace("C1", "C1&id=COTHER"),
            destination
                .launch()
                .replace("20.000001", "20.000001&thread_ts=1.0"),
            destination.launch().replacen('{', "{\"unknown\":true,", 1),
        ] {
            assert!(Activation::parse(&input).is_none(), "{input}");
        }
    }
    #[test]
    fn cold_start_keeps_latest_click_and_consumes_it_once_even_when_owner_rejected() {
        let (mut core, destination) = source();
        let mut pending = Pending::default();
        pending.push(Activation::Foreground);
        pending.push(Activation::Destination(destination));
        assert!(pending.take().is_none());
        pending.ready();
        core.session = None;
        let Some(Activation::Destination(destination)) = pending.take() else {
            panic!()
        };
        assert!(!destination.current(&core));
        assert!(pending.take().is_none());
    }
}
