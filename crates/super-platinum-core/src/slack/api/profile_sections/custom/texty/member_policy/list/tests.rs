use super::*;
use serde_json::json;

fn channel() -> ListChannel {
    ListChannel {
        is_im_or_mpim: false,
        unknown_or_nonexistent: false,
        externally_shared: false,
    }
}
fn assignments(keys: &[&str]) -> Value {
    let rows: serde_json::Map<String, Value> = keys
        .iter()
        .map(|key| ((*key).into(), json!({"group":"on"})))
        .collect();
    json!({"ok":true,"assignments":rows})
}
fn user(id: &str) -> User {
    serde_json::from_value(json!({"id":id})).unwrap()
}
fn allows(policy: &ListPolicy, user: &User, facts: ListMemberFacts) -> bool {
    policy.allows(user, Facts::default(), facts).unwrap()
}

#[test]
fn ordinary_list_expert_has_channel_gates_without_changing_profile() {
    let expert = user("UCHANNELEXPERT");
    for extra in [
        None,
        Some("ncea_allow_external_channels"),
        Some("ncea_allow_only_external_channel_usage"),
    ] {
        let mut keys = vec!["enable_native_channel_expert"];
        keys.extend(extra);
        let raw = assignments(&keys);
        for present in [false, true] {
            for im_or_mpim in [false, true] {
                for external in [false, true] {
                    let current = present.then_some(ListChannel {
                        is_im_or_mpim: im_or_mpim,
                        externally_shared: external,
                        ..channel()
                    });
                    let policy = ListPolicy::decode(&raw, current, None).unwrap();
                    assert_eq!(
                        allows(&policy, &expert, ListMemberFacts::default()),
                        !present || (!im_or_mpim && (!external || extra.is_some()))
                    );
                    assert!(
                        Policy::decode(&raw)
                            .unwrap()
                            .allows(&expert, Facts::default())
                            .unwrap()
                    );
                }
            }
        }
    }
    let off = ListPolicy::decode(&assignments(&[]), Some(channel()), None).unwrap();
    assert!(!allows(&off, &expert, ListMemberFacts::default()));
}

#[test]
fn external_bot_unknown_membership_is_allowed_but_known_nonmember_is_not() {
    let raw = assignments(&["at_menu_hide_external_bots"]);
    let current = ListChannel {
        externally_shared: true,
        ..channel()
    };
    let policy = ListPolicy::decode(&raw, Some(current), None).unwrap();
    let bot = user("U00000001");
    for (membership, expected) in [
        (ListMembership::Unknown, true),
        (ListMembership::Known(true), true),
        (ListMembership::Known(false), false),
    ] {
        assert_eq!(
            allows(
                &policy,
                &bot,
                ListMemberFacts {
                    external_bot: Some(true),
                    membership: Some(membership),
                }
            ),
            expected
        );
    }
    assert!(
        policy
            .allows(
                &bot,
                Facts::default(),
                ListMemberFacts {
                    external_bot: Some(true),
                    membership: None,
                }
            )
            .is_err()
    );
    assert!(
        policy
            .allows(&bot, Facts::default(), ListMemberFacts::default())
            .is_err()
    );
    assert!(allows(
        &policy,
        &bot,
        ListMemberFacts {
            external_bot: Some(false),
            membership: None,
        }
    ));
}

#[test]
fn external_bot_filter_has_exact_absent_unknown_and_internal_channel_branches() {
    let raw = assignments(&["at_menu_hide_external_bots"]);
    let bot = user("U00000001");
    for current in [
        None,
        Some(ListChannel {
            unknown_or_nonexistent: true,
            ..channel()
        }),
    ] {
        let policy = ListPolicy::decode(&raw, current, None).unwrap();
        assert!(allows(&policy, &bot, ListMemberFacts::default()));
    }
    let internal = ListPolicy::decode(&raw, Some(channel()), None).unwrap();
    assert!(!allows(
        &internal,
        &bot,
        ListMemberFacts {
            external_bot: Some(true),
            membership: None,
        }
    ));
    assert!(allows(
        &internal,
        &bot,
        ListMemberFacts {
            external_bot: Some(false),
            membership: None,
        }
    ));
    let off = ListPolicy::decode(&assignments(&[]), Some(channel()), None).unwrap();
    assert!(allows(&off, &bot, ListMemberFacts::default()));
}

#[test]
fn assistant_exclusion_uses_im_or_mpim_and_exact_original_id() {
    let actor = user("U00000001");
    for im_or_mpim in [false, true] {
        let policy = ListPolicy::decode(
            &assignments(&[]),
            Some(ListChannel {
                is_im_or_mpim: im_or_mpim,
                ..channel()
            }),
            Some(actor.id.clone()),
        )
        .unwrap();
        assert_eq!(policy.requirements(&actor).unwrap().eligible, !im_or_mpim);
        assert_eq!(
            allows(&policy, &actor, ListMemberFacts::default()),
            !im_or_mpim
        );
        assert!(allows(
            &policy,
            &user("U00000002"),
            ListMemberFacts::default()
        ));
    }
}

#[test]
fn extra_assignments_must_be_observed_and_shared_eligibility_remains_required() {
    for key in [
        "ncea_allow_external_channels",
        "ncea_allow_only_external_channel_usage",
        "at_menu_hide_external_bots",
    ] {
        let mut raw = assignments(&[]);
        raw["assignments"][key] = json!({"group":null});
        assert!(ListPolicy::decode(&raw, None, None).is_err());
        // These List-only assignments do not change the existing Profile API.
        assert!(Policy::decode(&raw).is_ok());
    }
    let policy = ListPolicy::decode(
        &assignments(&["loom_app_acl_search_filter", "app_acls_for_all_apps"]),
        None,
        None,
    )
    .unwrap();
    let app: User = serde_json::from_value(
        json!({"id":"U00000001","is_bot":true,"profile":{"api_app_id":"A00000001"}}),
    )
    .unwrap();
    assert!(policy.requirements(&app).unwrap().app_profile);
    assert!(
        policy
            .allows(&app, Facts::default(), ListMemberFacts::default())
            .is_err()
    );
    assert!(
        !policy
            .allows(
                &app,
                Facts {
                    restricted_app: Some(true),
                    ..Facts::default()
                },
                ListMemberFacts::default()
            )
            .unwrap()
    );
    let guest: User = serde_json::from_value(
        json!({"id":"U00000002","is_restricted":true,"restricted_user_type":"external_guest"}),
    )
    .unwrap();
    assert!(!policy.requirements(&guest).unwrap().eligible);
}
