use super::super::tests::{EDITOR, adopt, setup};
use super::*;
use serde_json::json;
use std::time::Duration;

#[test]
fn memberless_original_witness_survives_only_member_repair() {
    let (mut shell, model, ticket, token) = setup();
    let root_before = serde_json::to_value(model.root.as_ref().unwrap()).unwrap();
    let witness = Witness::capture(
        &shell,
        &model,
        ticket.clone(),
        model.root.clone().unwrap(),
        token,
    )
    .unwrap();
    let team = ticket.owner.scope.team.clone();
    shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .users
        .get_mut(EDITOR)
        .unwrap()
        .profile
        .as_mut()
        .unwrap()
        .title = Some("Hydrated profile title".into());
    assert!(member::Member::capture(&shell, &ticket.owner, EDITOR).is_err());
    assert!(member::Member::recoverable(&shell, &ticket.owner, EDITOR).is_ok());
    assert!(witness.current(&shell, &model));
    // Stand-in for the shared adopter in this pure guard test. It proves no
    // stale typed-member witness prevents checking the freshly admitted row;
    // actual request/adoption is covered by shared recovery/native fixtures.
    adopt(
        &mut shell,
        json!({"id":EDITOR,"profile":{"display_name":"Fresh editor","title":"Fresh title"}}),
    );
    assert!(witness.current(&shell, &model));
    assert_eq!(
        member::Member::capture(&shell, &ticket.owner, EDITOR)
            .unwrap()
            .raw["profile"]["display_name"],
        "Fresh editor"
    );
    assert_eq!(model.serial, ticket.serial);
    assert_eq!(
        serde_json::to_value(model.root.as_ref().unwrap()).unwrap(),
        root_before
    );
}

#[test]
fn repair_witness_refuses_original_source_owner_policy_and_slot_retirement() {
    for change in 0..10 {
        let (mut shell, mut model, ticket, token) = setup();
        let witness = Witness::capture(
            &shell,
            &model,
            ticket.clone(),
            model.root.clone().unwrap(),
            token,
        )
        .unwrap();
        let team = ticket.owner.scope.team.clone();
        match change {
            0 => {
                model
                    .root
                    .as_mut()
                    .unwrap()
                    .extra
                    .get_mut("document_comment")
                    .unwrap()["most_recent_editor"] = json!("UATTR0002")
            }
            1 => model.serial += 1,
            2 => model.query = Some(Query::Stream(super::super::super::Filter::Recent)),
            3 => model.attribution.token = Some(Arc::new(())),
            4 => shell.core.active_account = Some("foreign".into()),
            5 => shell.core.account_epoch += 1,
            6 => {
                shell
                    .core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut(&team)
                    .unwrap()
                    .token = "changed".into()
            }
            7 => shell.core.workspaces.get_mut(&team).unwrap().rt_generation += 1,
            8 => {
                let socket = shell.core.workspaces[&team].rt_generation;
                crate::app_home::realtime(
                    &mut shell,
                    &team,
                    socket,
                    &super_platinum_core::slack::events::RtEvent::PreferencesChanged {
                        name: "display_real_names_override".into(),
                        value: json!(1),
                    },
                );
            }
            _ => shell.overlay = None,
        }
        assert!(!witness.current(&shell, &model), "retirement case {change}");
    }
}

#[test]
fn recovery_requires_canonical_known_ordinary_selected_editor() {
    for raw in [
        json!({"id":EDITOR,"profile":{},"isUnknown":true}),
        json!({"id":EDITOR,"profile":{},"isNonExistent":true}),
        json!({"id":EDITOR,"profile":{},"isUnknown":"false"}),
        json!({"id":EDITOR,"profile":{},"is_bot":true}),
        json!({"id":EDITOR,"profile":{},"is_app_user":true}),
        json!({"id":EDITOR,"profile":{},"bot_id":"B1"}),
    ] {
        let (mut shell, _, ticket, _) = setup();
        adopt(&mut shell, raw);
        assert!(member::Member::recoverable(&shell, &ticket.owner, EDITOR).is_err());
    }
    let (mut shell, _, ticket, _) = setup();
    let team = ticket.owner.scope.team.clone();
    shell.core.workspaces.get_mut(&team).unwrap().users.insert(
        "UMISSING1".into(),
        serde_json::from_value(json!({"id":"UMISSING1","profile":{"display_name":"Typed only"}}))
            .unwrap(),
    );
    assert!(member::Member::recoverable(&shell, &ticket.owner, "UMISSING1").is_err());
    assert!(member::Member::recoverable(&shell, &ticket.owner, EDITOR).is_ok());
}

#[test]
fn shared_cooldown_preserves_deadline_and_indefinite_block_without_restart() {
    let now = Instant::now();
    let deadline = now + Duration::from_secs(2);
    let status = known::RecoveryStatus {
        retry_at: Some(Some(deadline)),
        failure: None,
    };
    assert!(cooling(&status, now));
    assert!(!cooling(&status, deadline));
    let failure = Failure::shared(
        Error::RateLimited {
            retry_after_secs: Some(15),
        },
        status,
    );
    assert_eq!(failure.retry_at, Some(deadline));
    assert!(!failure.blocked);
    let status = known::RecoveryStatus {
        retry_at: Some(None),
        failure: None,
    };
    assert!(cooling(&status, now + Duration::from_secs(60)));
    let failure = Failure::shared(
        Error::RateLimited {
            retry_after_secs: None,
        },
        status,
    );
    assert!(failure.blocked);
    assert!(failure.retry_at.is_none());
    let status = known::RecoveryStatus {
        retry_at: None,
        failure: None,
    };
    assert!(!cooling(&status, now));
    let failure = Failure::shared(Error::Offline("fixture offline".into()), status);
    assert!(matches!(failure.error, Error::Offline(_)));
    assert!(!failure.blocked);
}

#[tokio::test]
async fn retirement_drops_held_carrier_without_releasing_or_delivering_it() {
    use std::{
        cell::Cell,
        pin::Pin,
        task::{Context, Poll},
    };
    struct Held<'a> {
        current: &'a Cell<bool>,
        polled: &'a Cell<bool>,
        dropped: &'a Cell<bool>,
    }
    impl Future for Held<'_> {
        type Output = Result<(), Error>;
        fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
            self.polled.set(true);
            self.current.set(false);
            Poll::Pending
        }
    }
    impl Drop for Held<'_> {
        fn drop(&mut self) {
            self.dropped.set(true);
        }
    }
    let current = Cell::new(true);
    let polled = Cell::new(false);
    let dropped = Cell::new(false);
    let result = guarded(
        Held {
            current: &current,
            polled: &polled,
            dropped: &dropped,
        },
        || current.get(),
    )
    .await;
    assert!(result.is_err());
    assert!(polled.get());
    assert!(dropped.get());
    polled.set(false);
    dropped.set(false);
    assert!(
        guarded(
            Held {
                current: &current,
                polled: &polled,
                dropped: &dropped
            },
            || current.get()
        )
        .await
        .is_err()
    );
    assert!(!polled.get());
    assert!(dropped.get());
}
