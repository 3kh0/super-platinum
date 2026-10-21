use super::*;

#[test]
fn named_boot_enters_actual_canonical_member_queue() {
    let owner = WorkspaceSession {
        team_id: "T1".into(),
        user_id: SELF.into(),
        ..super::super::session()
    };
    let (boot, _) = seed_payloads(&owner);
    let mut members = super_platinum_core::state::directory_members::Store::default();
    members
        .observe_boot("fixture-account", &owner, &Default::default(), &boot)
        .unwrap();
    assert!(members.has_pending("fixture-account", &owner).unwrap());
}

#[test]
fn disconnected_identity_never_adopts_replaced_credentials_or_transport() {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    let team = shell.core.active_team.clone().unwrap();
    let session = WorkspaceSession {
        team_id: team.clone(),
        user_id: shell.self_account.user_id.clone(),
        enterprise_id: None,
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: TOKEN.into(),
    };
    let transport = Arc::new(Transport::disconnected_fixture(COOKIE));
    shell.core.transport = Some(transport.clone());
    shell.core.session = Some(Session {
        d_cookie: COOKIE.into(),
        unified_user_client_enabled: Default::default(),
        workspaces: [(team.clone(), session.clone())].into(),
    });
    let model = Model {
        transport: transport.clone(),
        session,
        account: shell.core.active_account.clone(),
        epoch: shell.core.account_epoch,
        ledger: Default::default(),
    };
    assert!(model.current(&shell));
    transport.set_health(Health::Online);
    assert!(model.current(&shell));
    shell.core.transport = Some(Arc::new(Transport::disconnected_fixture(COOKIE)));
    assert!(!model.current(&shell));
    shell.core.transport = Some(transport);
    shell.core.session.as_mut().unwrap().d_cookie = "replacement".into();
    assert!(!model.current(&shell));
    shell.core.session.as_mut().unwrap().d_cookie = COOKIE.into();
    shell
        .core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut(&team)
        .unwrap()
        .token = "replacement".into();
    assert!(!model.current(&shell));
    shell
        .core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut(&team)
        .unwrap()
        .token = TOKEN.into();
    shell.core.account_epoch += 1;
    assert!(!model.current(&shell));
    shell.core.account_epoch -= 1;
    shell.core.active_account = Some("replacement".into());
    assert!(!model.current(&shell));
}

#[test]
fn factory_admission_rejects_added_request_fields_and_unrelated_endpoints() {
    let client = super_platinum_core::slack::SlackClient::new(Default::default());
    let owner = super::super::session();
    let mut request =
        super_platinum_core::block_actions::name_policy::request::user(&client, &owner);
    assert!(protocol::validate(&client, &owner, &request).is_ok());
    let super_platinum_core::slack::client::RequestBody::Form(fields) = &mut request.body else {
        panic!("form")
    };
    fields.push(("extra".into(), "unadmitted".into()));
    assert!(protocol::validate(&client, &owner, &request).is_err());
    request.url = "https://example.test/api/chat.postMessage".into();
    assert!(protocol::validate(&client, &owner, &request).is_err());
}
