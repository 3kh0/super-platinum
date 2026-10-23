use super::super::tests::{prepared_with, publish_origin, receipt, staged_origin};
use super::*;
use serde_json::json;
const ID: &str = "CINIT0001";

fn prepared() -> (ShellState, Owner) {
    prepared_with(|shell| {
        // This sink test needs an actually configured boot before the original
        // request owner is captured. The shared fixture's legacy missing config
        // remains unchanged for other tests.
        let team = shell.core.active_team.clone().unwrap();
        let session = shell.core.session.as_mut().unwrap();
        session
            .unified_user_client_enabled
            .insert(team.clone(), false);
        let credential = session.workspaces[&team].clone();
        let transport = shell.core.transport.clone().unwrap();
        let boot = super_platinum_core::state::posting_metadata::authority::BootRead::begin(
            &mut shell.core,
            &transport,
            &credential,
        )
        .unwrap();
        let raw = json!({"ok":true,"team":{"id":team,"name":"Channel refresh"},
            "self":{"id":credential.user_id,"team_id":credential.team_id,"profile":{}},
            "read_only_channels":[],"thread_only_channels":[],
            "channels":[{"id":ID,"name":"warm-harbor","is_channel":true,"is_member":true,
                "is_read_only":false,"is_thread_only":false}]});
        assert!(boot.apply(&mut shell.core, raw, false).unwrap());
    })
}

fn queued(shell: &mut ShellState, owner: &Owner) -> (Completion, Value) {
    let team = &owner.context.session.team_id;
    let authority = Authority::capture(&shell.core, team).unwrap();
    let posting = authority.begin_channel_read(&mut shell.core, ID).unwrap();
    let raw = json!({"id":ID,"name":"refreshed","is_channel":true,"is_member":true,"is_read_only":true,"is_thread_only":false,"opaque":{"kept":[1,2]}});
    let mut direct = receipt(shell, owner);
    direct.origin = Origin::Direct;
    direct.requested = Some(ID.into());
    direct
        .accept(shell, std::slice::from_ref(&raw), &|_| true)
        .unwrap();
    let completion = Arc::new(Mutex::new(Status::Pending));
    let observed = observe(shell, &owner.context).unwrap();
    shell.app_home.channel_fetches.posting.push(Pending {
        owner: owner.clone(),
        observed,
        read: posting,
        raw: raw.clone(),
        completion: completion.clone(),
    });
    (completion, raw)
}
#[test]
fn posting_and_canonical_change_at_same_actual_sink() {
    let (mut shell, owner) = prepared();
    let (completion, raw) = queued(&mut shell, &owner);
    assert!(matches!(*completion.lock().unwrap(), Status::Pending));
    publish_origin(&mut shell, &owner, Origin::Direct);
    let done = completion.lock().unwrap();
    let Status::Applied(witness) = &*done else {
        panic!("paired sink refused")
    };
    assert!(witness.current(&shell.core));
    assert_eq!(witness.value(), &raw);
    let a = Authority::capture(&shell.core, &owner.context.session.team_id).unwrap();
    let flags = a
        .metadata(&shell.core)
        .unwrap()
        .effective(a.configuration(), &raw);
    assert_eq!(
        flags,
        Err(
            super_platinum_core::state::posting_metadata::Unknown::Policy(
                super_platinum_core::block_actions::posting_policy::Unknown::FeatureAccess
            )
        )
    );
    assert_eq!(witness.value()["is_read_only"], true);
    assert_eq!(witness.value()["is_thread_only"], false);
    let catalog = canonical::capture(&mut shell, &owner.context.session.team_id).unwrap();
    assert_eq!(catalog.row(ID).unwrap()["opaque"], raw["opaque"]);
    assert_eq!(catalog.row(ID).unwrap()["is_read_only"], true);
}
#[test]
fn newer_queued_same_channel_cannot_inherit_older_posting_grant() {
    let (mut shell, owner) = prepared();
    let (completion, _) = queued(&mut shell, &owner);
    let mut newer = receipt(&mut shell, &owner);
    newer.origin = Origin::Direct;
    newer.requested = Some(ID.into());
    newer
        .accept(
            &mut shell,
            &[json!({"id":ID,"name":"newer","is_read_only":false,"is_thread_only":true})],
            &|_| true,
        )
        .unwrap();
    publish_origin(&mut shell, &owner, Origin::Direct);
    assert!(matches!(*completion.lock().unwrap(), Status::Retired));
    let catalog = canonical::capture(&mut shell, &owner.context.session.team_id).unwrap();
    assert_eq!(catalog.row(ID).unwrap()["name"], "newer");
    assert_eq!(catalog.row(ID).unwrap()["is_thread_only"], true);
}
#[test]
fn account_socket_or_native_replacement_refuses_staged_pair() {
    for kind in 0..3 {
        let (mut shell, owner) = prepared();
        let (completion, _) = queued(&mut shell, &owner);
        let (_, native) = staged_origin(&shell, &owner, Origin::Direct);
        match kind {
            0 => shell.core.account_epoch += 1,
            1 => {
                shell
                    .core
                    .workspaces
                    .get_mut(&owner.context.session.team_id)
                    .unwrap()
                    .rt_generation += 1
            }
            _ => {
                shell
                    .core
                    .workspaces
                    .get_mut(&owner.context.session.team_id)
                    .unwrap()
                    .channels
                    .get_mut(ID)
                    .unwrap()
                    .extra
                    .insert("newer-event".into(), json!(true));
            }
        }
        assert!(super::prepare(&mut shell, &owner, &native).is_empty());
        assert!(matches!(*completion.lock().unwrap(), Status::Retired));
    }
}
