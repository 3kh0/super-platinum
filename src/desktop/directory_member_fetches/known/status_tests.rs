use super::*;
use super_platinum_core::{config::Session, state::Workspace};

fn setup() -> (ShellState, Arc<Context>) {
    let mut shell = ShellState::login(crate::media::MediaRegistry::default());
    let session = WorkspaceSession {
        team_id: "TRECOVER".into(),
        user_id: "USELF".into(),
        enterprise_id: None,
        name: "Synthetic".into(),
        url: "https://example.invalid".into(),
        token: "synthetic".into(),
    };
    shell.core.active_account = Some("synthetic-account".into());
    shell.core.session = Some(Session {
        d_cookie: "synthetic".into(),
        unified_user_client_enabled: BTreeMap::new(),
        workspaces: BTreeMap::from([(session.team_id.clone(), session.clone())]),
    });
    shell.core.transport = Some(Arc::new(Transport::disconnected_fixture("synthetic")));
    shell
        .core
        .workspaces
        .insert(session.team_id.clone(), Workspace::from_session(&session));
    let context = Arc::new(Context::capture(&shell, &session.team_id).unwrap());
    (shell, context)
}
fn install(shell: &mut ShellState, context: &Arc<Context>, deadline: Option<Instant>) {
    shell.directory_member_fetches.slots.insert(
        context.key(),
        Slot {
            context: context.clone(),
            manager: Default::default(),
            changed: tokio::sync::watch::channel(0).0,
            retry_at: Some(deadline),
            last_failure: Some(Arc::new(Error::RateLimited {
                retry_after_secs: Some(15),
            })),
        },
    );
}
#[test]
fn absent_feedback_does_not_create_a_recovery_slot() {
    let (shell, context) = setup();
    let feedback = recovery_status(&shell, &context.session.team_id).unwrap();
    assert!(feedback.retry_at.is_none() && feedback.failure.is_none());
    assert!(shell.directory_member_fetches.slots.is_empty());
}
#[test]
fn feedback_preserves_exact_shared_deadline_without_restarting_it() {
    let (mut shell, context) = setup();
    for deadline in [
        Some(Instant::now() + Duration::from_secs(15)),
        Some(Instant::now()),
        None,
    ] {
        install(&mut shell, &context, deadline);
        for _ in 0..2 {
            let feedback = recovery_status(&shell, &context.session.team_id).unwrap();
            assert_eq!(feedback.retry_at, Some(deadline));
            assert!(matches!(
                feedback.failure,
                Some(Error::RateLimited {
                    retry_after_secs: Some(15)
                })
            ));
        }
    }
}
#[test]
fn retired_carrier_credentials_or_socket_cannot_lend_feedback() {
    for kind in 0..3 {
        let (mut shell, context) = setup();
        install(&mut shell, &context, Some(Instant::now()));
        match kind {
            0 => shell.core.transport = Some(Arc::new(Transport::disconnected_fixture("new"))),
            1 => shell.core.session.as_mut().unwrap().d_cookie = "new".into(),
            _ => {
                shell
                    .core
                    .workspaces
                    .get_mut(&context.session.team_id)
                    .unwrap()
                    .rt_generation += 1
            }
        }
        assert!(recovery_status(&shell, &context.session.team_id).is_err());
    }
}
