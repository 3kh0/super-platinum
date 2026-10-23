use super::*;
use serde_json::json;

fn setup() -> (ShellState, Arc<Context>) {
    let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
    fixture::install(&mut shell);
    let team = shell.core.active_team.clone().unwrap();
    let context = Arc::new(Context::capture(&shell, &team).unwrap());
    (shell, context)
}

fn prepared(context: Arc<Context>, raw: Value) -> Arc<Prepared> {
    let version = selected_version(&raw).unwrap();
    Arc::new(Prepared {
        context,
        version,
        raw,
        standard: StandardEmoji::build(version).unwrap(),
    })
}

fn install(shell: &mut ShellState, context: Arc<Context>, value: Arc<Prepared>) -> Owner {
    let mut transformations = members::Transformations::decode(&value.raw).unwrap();
    transformations.emoji = Some(value);
    let owner = initial::begin(shell, context, Arc::new(transformations), true, false).unwrap();
    initial::retire(shell, &owner);
    owner
}

#[test]
fn shared_emoji_complete_assignment_is_required_and_strict_on_selects_v17() {
    for (raw, expected) in [
        (json!({"ok":true,"assignments":{}}), Some(Version::V16)),
        (
            json!({"ok":true,"assignments":{"emoji_v17":null}}),
            Some(Version::V16),
        ),
        (
            json!({"ok":true,"assignments":{"emoji_v17":{"group":"on"}}}),
            Some(Version::V17),
        ),
        (
            json!({"ok":true,"assignments":{"emoji_v17":{"group":"treatment"}}}),
            Some(Version::V16),
        ),
        (
            json!({"ok":true,"assignments":{"emoji_v17":{"group":true}}}),
            None,
        ),
        (json!({"ok":true}), None),
        (json!({"ok":false,"assignments":{}}), None),
        (json!({"ok":true,"error":"denied","assignments":{}}), None),
    ] {
        assert_eq!(selected_version(&raw), expected, "{raw}");
    }
}

#[test]
fn shared_emoji_unobserved_slot_does_not_invent_catalog() {
    let (mut shell, context) = setup();
    let team = context.session.team_id.clone();
    assert!(emoji_standard(&shell, &team).is_none());
    let raw = json!({"ok":true,"assignments":{"emoji_v17":{"group":false}}});
    let owner = initial::begin(
        &mut shell,
        context,
        Arc::new(members::Transformations::decode(&raw).unwrap()),
        true,
        false,
    )
    .unwrap();
    assert!(
        owner.current(&shell),
        "optional invalid emoji assignment does not fail unrelated initialization"
    );
    assert!(emoji_standard(&shell, &team).is_none());
}

#[test]
fn shared_emoji_first_accepted_map_survives_same_context_resync() {
    let (mut shell, context) = setup();
    let team = context.session.team_id.clone();
    let first_raw = json!({"ok":true,"assignments":{}});
    install(
        &mut shell,
        context.clone(),
        prepared(context.clone(), first_raw.clone()),
    );
    let old = emoji_standard(&shell, &team).unwrap();
    assert_eq!(
        old.standard().unicode_name("1f44d-1f3fd"),
        Some("+1::skin-tone-4")
    );
    install(
        &mut shell,
        context.clone(),
        prepared(
            context,
            json!({"ok":true,"assignments":{"emoji_v17":{"group":"on"}}}),
        ),
    );
    assert!(
        !old.current(&shell),
        "old slot handle retires even when map is retained"
    );
    let current = emoji_standard(&shell, &team).unwrap();
    assert_eq!(current.version(), Version::V16);
    assert_eq!(current.response(), &first_raw);
    assert_eq!(current.standard().unicode_name("1facd"), None);
    assert!(Arc::ptr_eq(&old.prepared, &current.prepared));
}

#[test]
fn shared_emoji_socket_rebind_cannot_relabel_old_response_as_current() {
    let (mut shell, context) = setup();
    let team = context.session.team_id.clone();
    install(
        &mut shell,
        context.clone(),
        prepared(context, json!({"ok":true,"assignments":{}})),
    );
    let old = emoji_standard(&shell, &team).unwrap();
    let generation = shell.core.workspaces[&team].rt_generation + 1;
    shell.core.workspaces.get_mut(&team).unwrap().rt_generation = generation;
    let _ = events::connected(&mut shell, &team, generation);
    assert!(!old.current(&shell));
    assert!(emoji_standard(&shell, &team).is_none());
    let fresh = Arc::new(Context::capture(&shell, &team).unwrap());
    install(
        &mut shell,
        fresh.clone(),
        prepared(
            fresh,
            json!({"ok":true,"assignments":{"emoji_v17":{"group":"on"}}}),
        ),
    );
    let current = emoji_standard(&shell, &team).unwrap();
    assert_eq!(current.version(), Version::V17);
    assert_eq!(current.standard().unicode_name("1facd"), Some("orca"));
}

#[test]
fn shared_emoji_original_account_and_auth_replacement_retire() {
    let (mut shell, context) = setup();
    let team = context.session.team_id.clone();
    install(
        &mut shell,
        context.clone(),
        prepared(context, json!({"ok":true,"assignments":{}})),
    );
    let old = emoji_standard(&shell, &team).unwrap();
    shell.core.account_epoch += 1;
    assert!(!old.current(&shell));
    assert!(emoji_standard(&shell, &team).is_none());
    shell.core.account_epoch -= 1;
    shell
        .core
        .session
        .as_mut()
        .unwrap()
        .workspaces
        .get_mut(&team)
        .unwrap()
        .token = "different-synthetic-token".into();
    assert!(!old.current(&shell));
    assert!(emoji_standard(&shell, &team).is_none());
}

#[test]
fn shared_emoji_late_prepared_original_cannot_install_under_new_context() {
    let (mut shell, context) = setup();
    let team = context.session.team_id.clone();
    let original = prepared(
        context,
        json!({"ok":true,"assignments":{"emoji_v17":{"group":"on"}}}),
    );
    shell.core.workspaces.get_mut(&team).unwrap().rt_generation += 1;
    let fresh = Arc::new(Context::capture(&shell, &team).unwrap());
    install(&mut shell, fresh, original);
    assert!(emoji_standard(&shell, &team).is_none());
}
