use super::*;
use serde_json::json;

fn context<'a>(team: &'a Value, member: &'a Value) -> Context<'a> {
    Context {
        team_id: "T1",
        top_level_team_id: "E1",
        user_id: "U1",
        team,
        member,
        unified: false,
        manage_posting_access: Some(false),
        channel_admin: None,
    }
}
fn channel() -> Value {
    json!({"id":"C1","is_channel":true,"is_ext_shared":false})
}

#[test]
fn free_and_standard_downgrade_ignore_restrictions_except_general() {
    let member = json!({"id":"U1"});
    for team in [json!({"id":"T1"}), json!({"id":"T1","plan":"std"})] {
        let ctx = context(&team, &member);
        let mut c = channel();
        let observed = Observed::from_inventory("C1".into(), true, true);
        assert!(observed.effective(&c, &ctx).unwrap().permits_root_post());
        c["is_general"] = json!(true);
        assert_eq!(
            observed.effective(&c, &ctx),
            Ok(Flags {
                read_only: true,
                thread_only: true
            })
        );
    }
}
#[test]
fn feature_access_and_external_host_preserve_free_team_restriction() {
    let team = json!({"id":"T1"});
    let member = json!({"id":"U1"});
    let mut ctx = context(&team, &member);
    let mut c = channel();
    let observed = Observed::from_inventory("C1".into(), true, false);
    ctx.manage_posting_access = Some(true);
    assert!(!observed.effective(&c, &ctx).unwrap().permits_root_post());
    ctx.manage_posting_access = None;
    assert_eq!(observed.effective(&c, &ctx), Err(Unknown::FeatureAccess));
    c["is_ext_shared"] = json!(true);
    c["conversation_host_id"] = json!("TOTHER");
    assert!(!observed.effective(&c, &ctx).unwrap().permits_root_post());
    c["conversation_host_id"] = json!("E1");
    assert_eq!(observed.effective(&c, &ctx), Err(Unknown::FeatureAccess));
}
#[test]
fn enterprise_workspace_and_paid_plans_do_not_get_free_exception() {
    let member = json!({"id":"U1"});
    let c = channel();
    for team in [
        json!({"id":"T1","enterprise_id":"E1"}),
        json!({"id":"T1","plan":"plus"}),
    ] {
        let ctx = context(&team, &member);
        assert!(
            !Observed::from_inventory("C1".into(), true, false)
                .effective(&c, &ctx)
                .unwrap()
                .permits_root_post()
        );
    }
}
#[test]
fn branch_order_keeps_free_im_feature_restriction_but_normalizes_paid_im() {
    let member = json!({"id":"U1"});
    let mut c = channel();
    c["is_im"] = json!(true);
    let observed = Observed::from_inventory("C1".into(), true, false);
    let free = json!({"id":"T1"});
    let paid = json!({"id":"T1","plan":"plus"});
    let mut ctx = context(&free, &member);
    ctx.manage_posting_access = Some(true);
    assert!(!observed.effective(&c, &ctx).unwrap().permits_root_post());
    assert!(
        observed
            .effective(&c, &context(&paid, &member))
            .unwrap()
            .permits_root_post()
    );
}
#[test]
fn explicit_user_override_precedes_external_host_and_role() {
    let team = json!({"id":"T1"});
    let member = json!({"id":"U1","is_restricted":true});
    let ctx = context(&team, &member);
    let mut c = channel();
    c["is_ext_shared"] = json!(true);
    c["conversation_host_id"] = json!("TOTHER");
    let pref = json!({"user":["U1"],"role":["admin"]});
    assert_eq!(
        preference_flags(&c, Some(&pref), Some(&pref), &ctx),
        Ok(Flags {
            read_only: false,
            thread_only: false
        })
    );
}
#[test]
fn regular_role_rejects_guest_and_external_host() {
    let team = json!({"id":"T1"});
    let mut member = json!({"id":"U1","is_restricted":false});
    let mut c = channel();
    let pref = json!({"role":["regular"]});
    assert!(
        preference_flags(&c, Some(&pref), Some(&pref), &context(&team, &member))
            .unwrap()
            .permits_root_post()
    );
    member["is_restricted"] = json!(true);
    assert!(
        !preference_flags(&c, Some(&pref), Some(&pref), &context(&team, &member))
            .unwrap()
            .permits_root_post()
    );
    member["is_restricted"] = json!(false);
    c["is_ext_shared"] = json!(true);
    c["conversation_host_id"] = json!("TOTHER");
    assert!(
        !preference_flags(&c, Some(&pref), Some(&pref), &context(&team, &member))
            .unwrap()
            .permits_root_post()
    );
}
#[test]
fn admin_requires_exact_channel_scoped_observation_not_global_admin() {
    let team = json!({"id":"T1"});
    let member = json!({"id":"U1","is_admin":true});
    let c = channel();
    let pref = json!({"role":["admin"]});
    let mut ctx = context(&team, &member);
    assert_eq!(
        preference_flags(&c, Some(&pref), Some(&pref), &ctx),
        Err(Unknown::ChannelAdmin)
    );
    ctx.channel_admin = Some(ChannelAdmin {
        channel_id: "C1",
        user_id: "U1",
        allowed: true,
    });
    assert!(
        preference_flags(&c, Some(&pref), Some(&pref), &ctx)
            .unwrap()
            .permits_root_post()
    );
    ctx.channel_admin = Some(ChannelAdmin {
        channel_id: "C2",
        user_id: "U1",
        allowed: true,
    });
    assert_eq!(
        preference_flags(&c, Some(&pref), Some(&pref), &ctx),
        Err(Unknown::Identity)
    );
}
#[test]
fn org_admin_uses_enterprise_member_role_and_first_role_only() {
    let team = json!({"id":"T1"});
    let member = json!({"id":"U1","enterprise_user":{"is_admin":true}});
    let c = channel();
    let ctx = context(&team, &member);
    let pref = json!({"type":["org_admin","regular"]});
    assert!(
        preference_flags(&c, Some(&pref), Some(&pref), &ctx)
            .unwrap()
            .permits_root_post()
    );
    let unknown = json!({"role":["future","all"]});
    assert_eq!(
        preference_flags(&c, Some(&unknown), None, &ctx),
        Err(Unknown::Shape)
    );
}
#[test]
fn denied_post_and_allowed_reply_is_thread_only() {
    let team = json!({"id":"T1"});
    let member = json!({"id":"U1"});
    let c = channel();
    assert_eq!(
        preference_flags(&c, Some(&json!({})), None, &context(&team, &member)),
        Ok(Flags {
            read_only: false,
            thread_only: true
        })
    );
    assert_eq!(
        preference_flags(
            &c,
            Some(&json!({})),
            Some(&json!({})),
            &context(&team, &member)
        ),
        Ok(Flags {
            read_only: true,
            thread_only: false
        })
    );
}
#[test]
fn partial_effective_ingress_preserves_omission_and_retires_malformed_companion() {
    let team = json!({"id":"T1","plan":"plus"});
    let member = json!({"id":"U1"});
    let ctx = context(&team, &member);
    let mut observed = Observed::from_inventory("C1".into(), true, true);
    observed
        .observe(&json!({"id":"C1","is_read_only":false}), &ctx)
        .unwrap();
    assert_eq!(observed.values(), (Some(false), Some(true)));
    observed
        .observe(
            &json!({"id":"C1","is_read_only":true,"is_thread_only":"false"}),
            &ctx,
        )
        .unwrap();
    assert_eq!(observed.values(), (Some(true), None));
    assert_eq!(
        observed.effective(&channel(), &ctx),
        Err(Unknown::EffectiveFlags)
    );
}
#[test]
fn effective_booleans_precede_preference_objects_and_unknown_never_grants() {
    let team = json!({"id":"T1","plan":"plus"});
    let member = json!({"id":"U1"});
    let ctx = context(&team, &member);
    let mut observed = Observed::unknown("C1".into());
    observed.observe(&channel(), &ctx).unwrap();
    assert_eq!(
        observed.effective(&channel(), &ctx),
        Err(Unknown::EffectiveFlags)
    );
    observed.observe(&json!({"id":"C1","is_read_only":false,"is_thread_only":false,"properties":{"posting_restricted_to":{}}}),&ctx).unwrap();
    assert!(
        observed
            .effective(&channel(), &ctx)
            .unwrap()
            .permits_root_post()
    );
}
#[test]
fn foreign_identity_cannot_replace_retained_flags() {
    let team = json!({"id":"T1"});
    let member = json!({"id":"U1"});
    let ctx = context(&team, &member);
    let mut observed = Observed::from_inventory("C1".into(), true, true);
    let before = observed.clone();
    assert_eq!(
        observed.observe(&json!({"id":"C2","is_read_only":false}), &ctx),
        Err(Unknown::Identity)
    );
    assert_eq!(observed, before);
}
#[test]
fn malformed_preference_retires_old_grant_and_file_huddle_cannot_root_post() {
    let team = json!({"id":"T1"});
    let member = json!({"id":"U1"});
    let ctx = context(&team, &member);
    let mut observed = Observed::from_inventory("C1".into(), false, false);
    assert_eq!(
        observed.observe(
            &json!({"id":"C1","properties":{"posting_restricted_to":{"role":[42]}}}),
            &ctx
        ),
        Err(Unknown::Shape)
    );
    assert_eq!(observed.values(), (None, None));
    for c in [
        json!({"id":"C1","is_file":true}),
        json!({"id":"C1","properties":{"huddles":{"is_huddle_channel":true}}}),
    ] {
        assert!(!observed.effective(&c, &ctx).unwrap().permits_root_post());
    }
}
