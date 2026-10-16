//! Unit tests only: shared builder/decoder runs, only final HTTP is substituted.
use super::*;
impl Binding {
    pub(crate) fn retained_route_test(account: &str) -> Self {
        Self {
            account: account.into(),
            epoch: 7,
            cookie: "fixture-cookie".into(),
            transport: Arc::new(Transport::new("fixture-cookie").unwrap()),
            client: SlackClient::default(),
            workspace: WorkspaceSession {
                team_id: "T1".into(),
                enterprise_id: None,
                user_id: "U1".into(),
                name: "Fixture".into(),
                url: "https://fixture.slack.com".into(),
                token: "fixture-token".into(),
            },
            cooldown_until_ms: Arc::new(Mutex::new(0)),
        }
    }
}
