use super::*;

#[test]
fn list_context_requires_current_account_team_pin_scope_and_transport_identity() {
    for change in 0..4 {
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        let transport = Arc::new(Transport::new("synthetic-pin-test-cookie").unwrap());
        shell.core.transport = Some(transport.clone());
        shell.core.pins.ensure_scope(shell.core.account_epoch, "T1");
        let ctx = Context {
            transport,
            client: shell.core.client.clone(),
            epoch: shell.core.account_epoch,
            session: WorkspaceSession {
                team_id: "T1".into(),
                enterprise_id: None,
                user_id: "U0".into(),
                name: "Fixture".into(),
                url: "https://example.invalid".into(),
                token: String::new(),
            },
        };
        assert!(ctx.current(&shell));
        match change {
            0 => shell.core.account_epoch += 1,
            1 => shell.core.active_team = Some("T2".into()),
            2 => shell.core.pins.ensure_scope(ctx.epoch, "T2"),
            _ => {
                shell.core.transport = Some(Arc::new(
                    Transport::new("synthetic-pin-test-cookie").unwrap(),
                ))
            }
        }
        assert!(!ctx.current(&shell));
    }
}
