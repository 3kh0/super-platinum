use super::test_support::*;
use super::*;
use crate::app_home::channel_fetches::calendar_member_test_support as renderer;
use std::{cell::Cell, collections::VecDeque};
use super_platinum_core::slack::client::RequestBody;

#[tokio::test]
async fn later_member_raw100_enters_actual_source_and_selects_canonical_current_values_only() {
    let f = setup().await;
    let state = f.h.state;
    let id = f.id;
    let ctx = &f.ctx;
    let baseline = f.renderer.requests().len();
    let (tx1, rx1) = tokio::sync::oneshot::channel();
    let (tx2, rx2) = tokio::sync::oneshot::channel();
    let mut receive = VecDeque::from([rx1, rx2]);
    let calls = Cell::new(0);
    let mut work = Box::pin(run_with(
        state,
        id,
        17,
        ctx,
        " Member ",
        |request, scope| {
            assert!(scope.admitted());
            let RequestBody::Json(body) = &request.body else {
                panic!("JSON")
            };
            let call = calls.get();
            calls.set(call + 1);
            assert_eq!(body["query"], " Member ");
            assert_eq!(body["count"], 100);
            assert_eq!(body["include_profile_only_users"], true);
            assert_eq!(body["search_email"], true);
            if call == 0 {
                assert!(body.get("marker").is_none());
            } else {
                assert_eq!(body["marker"], "next");
            }
            let response = receive.pop_front().unwrap();
            async move {
                scope
                    .injected(request, |_| async move { response.await.unwrap() })
                    .await
            }
        },
    ));
    renderer::pending(&f.h._dom, work.as_mut()).await;
    tx1.send(Ok(
        json!({"results":vec![row("UDUPMEM01","Member Duplicate",101);99],"next_marker":"next"}),
    ))
    .unwrap();
    renderer::pending(&f.h._dom, work.as_mut()).await;
    assert!(
        !state.peek().core.workspaces[&ctx.session.team_id]
            .users
            .contains_key("UDUPMEM01")
    );
    tx2.send(Ok(json!({"results":[row("ULATER001","Member Later",101),null,{"id":"UROW00101","profile":false}],"next_marker":{"unused":true}}))).unwrap();
    let acquired = renderer::run(&f.h._dom, work).await.unwrap().unwrap();
    assert_eq!(calls.get(), 2);
    assert_eq!(acquired.ids, ["UDUPMEM01", "ULATER001"]);
    assert!(f.renderer.requests().len() > baseline);
    // The actual producer registered every ID before intake. Publication must
    // refuse the complete bundle if that guard is absent, without partial hits.
    {
        let mut state = state;
        let mut s = state.write();
        let mut first_only = origins::Origins::default();
        let registration = first_only
            .propose(&HashMap::new(), &acquired.ids[..1])
            .unwrap();
        first_only.commit(registration);
        let origins = std::mem::replace(
            &mut active_mut(&mut s, id).unwrap().members.origins,
            first_only,
        );
        assert!(
            active(&s, id)
                .unwrap()
                .members
                .origins
                .guarded(&acquired.ids[0])
        );
        assert!(
            !active(&s, id)
                .unwrap()
                .members
                .origins
                .guarded(&acquired.ids[1])
        );
        assert!(active(&s, id).unwrap().members.facts.is_none());
        let mut remote = Vec::new();
        assert!(!append(
            &mut s,
            id,
            17,
            ctx,
            Acquired {
                source: acquired.source.clone(),
                ids: acquired.ids.clone(),
                profiles: acquired.profiles.clone(),
            },
            &mut remote,
        ));
        assert!(remote.is_empty());
        assert!(active(&s, id).unwrap().members.hits.is_empty());
        assert!(active(&s, id).unwrap().members.facts.is_none());
        active_mut(&mut s, id).unwrap().members.origins = origins;
    }
    let choices = f.publish(acquired);
    assert!(choices.iter().any(|v| v == "ULATER001"));
    assert!(!choices.iter().any(|v| v == "UROW00101"));
    super::super::super::actions::edit(
        state,
        id,
        super::super::super::actions::Field::ShareChannel,
        "ULATER001".into(),
    );
    assert_eq!(
        active(&state.peek(), id).unwrap().share_channel,
        "ULATER001"
    );
    assert_eq!(
        super::super::resolve(&state.peek(), id).as_deref(),
        Some("")
    );
    f.new_query(18, "Member");
    let mut stranger = row("UBLOCK001", "Member Blocked", 101);
    stranger["is_stranger"] = json!(true);
    let mut profile = row("UPROFILE1", "Member Profile", 101);
    profile["is_profile_only_user"] = json!(true);
    let acquired = f
        .acquire(vec![
            row("ULATER001", "Member Raw Spoof", 101),
            stranger,
            profile,
        ])
        .await;
    let choices = f.publish(acquired);
    let s = state.peek();
    let c = active(&s, id).unwrap();
    assert_eq!(c.share_channel, "ULATER001");
    assert!(choices.iter().any(|v| v == "ULATER001"));
    assert!(!choices.iter().any(|v| v == "UBLOCK001"));
    assert!(!choices.iter().any(|v| v == "UPROFILE1"));
    assert_eq!(
        s.core.workspaces[&ctx.session.team_id].users["ULATER001"]
            .name
            .as_deref(),
        Some("Member Later")
    );
    assert_eq!(c.notice.as_deref(), Some("Unrelated preference warning"));
    assert_eq!(
        c.members
            .facts
            .as_ref()
            .unwrap()
            .rows()
            .iter()
            .find(|row| row.source["id"] == "UBLOCK001")
            .unwrap()
            .interaction,
        Some(false)
    );
    drop(s);
    // The observed on branch genuinely normalizes visible external users.
    // Verify its positive behavior separately, preserving the off-branch denial.
    let normalized = setup_with_normalize(true).await;
    let mut incoming = row("UNORM0001", "Member Normalized", 101);
    incoming["is_stranger"] = json!(true);
    let acquired = normalized.acquire(vec![incoming]).await;
    let visible = normalized.publish(acquired);
    assert!(visible.iter().any(|id| id == "UNORM0001"));
    let normalized_state = normalized.h.state.peek();
    let current = active(&normalized_state, normalized.id)
        .unwrap()
        .members
        .facts
        .as_ref()
        .unwrap()
        .rows()
        .iter()
        .find(|row| row.source["id"] == "UNORM0001")
        .unwrap();
    assert_eq!(current.interaction, Some(true));
    assert_eq!(current.source["is_stranger"], json!(false));
}

#[tokio::test]
async fn later_failure_bad_prefix_missing_flags_and_issued_retired_429_preserve_owned_state() {
    for case in 0..4 {
        let f = setup().await;
        let state = f.h.state;
        let id = f.id;
        let ctx = &f.ctx;
        let before = f.source();
        let users = serde_json::to_value(&state.peek().core.workspaces[&ctx.session.team_id].users)
            .unwrap();
        let calls = Cell::new(0);
        let (send, recv) = tokio::sync::oneshot::channel();
        let mut receiver = Some(recv);
        if case == 3 {
            let mut state = state;
            renderer::retire_member_flags(&mut state.write());
        }
        let mut work = Box::pin(run_with(state, id, 17, ctx, "Member", |request, scope| {
            let n = calls.get();
            calls.set(n + 1);
            let receiver = if n == 1 { receiver.take() } else { None };
            async move {
                scope.injected(request,|_|async move {
                if let Some(receiver)=receiver {return receiver.await.unwrap();}
                if case==2 {return Ok(json!({"results":[{"id":"UBADSEED1","profile":false}]}));}
                Ok(json!({"results":[row("UPARTIAL1","Member Partial",101)],"next_marker":"next"}))
            }).await
            }
        }));
        if case < 2 {
            renderer::pending(&f.h._dom, work.as_mut()).await;
            assert_eq!(calls.get(), 2);
            if case == 1 {
                let mut state = state;
                active_mut(&mut state.write(), id).unwrap().share_generation += 1;
            }
            send.send(Err(if case == 1 {
                Error::RateLimited {
                    retry_after_secs: Some(37),
                }
            } else {
                Error::Transport("second page failed".into())
            }))
            .unwrap();
        }
        let result = renderer::run(&f.h._dom, work).await;
        if case == 1 {
            assert!(result.unwrap().is_none());
        } else {
            assert!(result.is_err());
        }
        assert_eq!(f.source(), before);
        assert_eq!(
            serde_json::to_value(&state.peek().core.workspaces[&ctx.session.team_id].users)
                .unwrap(),
            users
        );
        assert!(
            !state.peek().core.workspaces[&ctx.session.team_id]
                .users
                .contains_key("UPARTIAL1")
        );
        if case == 1 {
            let s = state.peek();
            let until = s.message_buttons.shortcuts.cooldowns
                [&(ctx.epoch, ctx.session.team_id.clone())]
                .unwrap();
            assert!(until.saturating_duration_since(Instant::now()) > Duration::from_secs(30));
        }
        if case == 3 {
            assert_eq!(calls.get(), 0);
        }
        assert_eq!(
            active(&state.peek(), id).unwrap().notice.as_deref(),
            Some("Unrelated preference warning")
        );
    }
}

#[tokio::test]
async fn renderer_and_post_adoption_retirement_keep_origins_and_stale_selection_never_uses_remote_cache_fallback()
 {
    let f = setup().await;
    let state = f.h.state;
    let id = f.id;
    let ctx = &f.ctx;
    let before = f.source();
    let held = f.renderer.hold_next();
    let mut work = Box::pin(run_with(
        state,
        id,
        17,
        ctx,
        "Member",
        |request, scope| async move {
            scope
                .injected(request, |_| async move {
                    Ok(json!({"results":[row("URENDER01","Member Renderer",101)]}))
                })
                .await
        },
    ));
    renderer::pending(&f.h._dom, work.as_mut()).await;
    assert!(
        active(&state.peek(), id)
            .unwrap()
            .members
            .origins
            .guarded("URENDER01")
    );
    assert!(
        !state.peek().core.workspaces[&ctx.session.team_id]
            .users
            .contains_key("URENDER01")
    );
    f.new_query(18, "Member");
    let request = f.renderer.requests().last().unwrap().clone();
    held.send(renderer::Renderer::reply(&request)).unwrap();
    assert!(renderer::run(&f.h._dom, work).await.is_err());
    assert_eq!(f.source(), before);
    let acquired = f
        .acquire(vec![row("UADOPT001", "Member Adopted", 101)])
        .await;
    assert!(
        state.peek().core.workspaces[&ctx.session.team_id]
            .users
            .contains_key("UADOPT001")
    );
    f.new_query(19, "Member");
    let retained = active(&state.peek(), id)
        .unwrap()
        .members
        .facts
        .as_ref()
        .unwrap()
        .clone();
    let mut remote = Vec::new();
    let mut write = state;
    assert!(!append(
        &mut write.write(),
        id,
        18,
        ctx,
        acquired,
        &mut remote
    ));
    assert!(remote.is_empty());
    assert!(Arc::ptr_eq(
        active(&state.peek(), id)
            .unwrap()
            .members
            .facts
            .as_ref()
            .unwrap(),
        &retained
    ));
    assert!(active(&state.peek(), id).unwrap().members.hits.is_empty());
    assert!(
        active(&state.peek(), id)
            .unwrap()
            .share_choices
            .iter()
            .any(|c| c.id == "UADOPT001")
    );
    {
        let mut s = write.write();
        s.core
            .workspaces
            .get_mut(&ctx.session.team_id)
            .unwrap()
            .users
            .get_mut("UADOPT001")
            .unwrap()
            .profile
            .as_mut()
            .expect("actual acquired profile")
            .title = Some("Native drift".into());
    }
    let selected = active(&state.peek(), id).unwrap().share_channel.clone();
    super::super::super::actions::edit(
        state,
        id,
        super::super::super::actions::Field::ShareChannel,
        "UADOPT001".into(),
    );
    assert_eq!(active(&state.peek(), id).unwrap().share_channel, selected);

    let f = setup().await;
    let state = f.h.state;
    let id = f.id;
    let ctx = &f.ctx;
    f.external(vec![row("UINDEPEND", "Member Independent", 101)])
        .await;
    let mut stranger = row("UINDEPEND", "Member Independent", 102);
    stranger["is_stranger"] = json!(true);
    let acquired = f.acquire(vec![stranger]).await;
    let choices = f.publish(acquired);
    assert!(choices.iter().any(|v| v == "UINDEPEND"));
    assert!(
        !active(&state.peek(), id)
            .unwrap()
            .share_remote
            .iter()
            .any(|v| v.id() == "UINDEPEND")
    );
    let stale: User = serde_json::from_value(row("UINDEPEND", "Stale remote object", 102)).unwrap();
    let mut current = state;
    active_mut(&mut current.write(), id)
        .unwrap()
        .share_remote
        .push(Destination::User(stale));
    super::super::super::actions::edit(
        state,
        id,
        super::super::super::actions::Field::ShareChannel,
        "UINDEPEND".into(),
    );
    let s = state.peek();
    let c = active(&s, id).unwrap();
    assert_eq!(c.share_channel, "UINDEPEND");
    assert!(c.share_selected.is_none());
    assert_eq!(super::super::resolve(&s, id).as_deref(), Some(""));
    assert!(ctx.session.team_id == s.core.active_team.clone().unwrap());
}

#[tokio::test]
async fn guarded_cache_is_filtered_before_original_cap_and_registry_or_id_budget_refusal_installs_nothing()
 {
    let f = setup().await;
    let state = f.h.state;
    let id = f.id;
    let ctx = &f.ctx;
    let guarded: Vec<_> = (0..50)
        .map(|n| row(&format!("UGUARDED{n:04}"), &format!("Member A{n:04}"), 101))
        .collect();
    let acquired = f.acquire(guarded).await;
    f.publish(acquired);
    {
        let mut current = state;
        let mut s = current.write();
        let w = s.core.workspaces.get_mut(&ctx.session.team_id).unwrap();
        // Independent legacy cache additions grant no canonical readiness.
        for n in 0..150 {
            let user: User = serde_json::from_value(row(
                &format!("ULEGACY{n:04}"),
                &format!("Member B{n:04}"),
                101,
            ))
            .unwrap();
            w.users.insert(user.id.clone(), user);
        }
    }
    let selected = active(&state.peek(), id).unwrap().share_channel.clone();
    let legacy = load::share_choices(&state.peek(), ctx, "Member", &selected);
    let admitted = choices(&state.peek(), id, ctx, "Member", &selected);
    assert_eq!(admitted.len(), 100);
    assert!(legacy.iter().any(|c| c.id == "UGUARDED0000"));
    assert!(!admitted.iter().any(|c| c.id.starts_with("UGUARDED")));
    assert!(!legacy.iter().any(|c| c.id == "ULEGACY0098"));
    assert!(admitted.iter().any(|c| c.id == "ULEGACY0098"));
    let extra: Vec<_> = (0..4046).map(|n| format!("UREGISTRY{n:04}")).collect();
    {
        let mut current = state;
        let mut s = current.write();
        let c = active_mut(&mut s, id).unwrap();
        let registration = c.members.origins.propose(&HashMap::new(), &extra).unwrap();
        c.members.origins.commit(registration);
        assert!(c.members.origins.guarded(extra.last().unwrap()));
        assert!(
            c.members
                .origins
                .propose(&HashMap::new(), &["UOVERFLOW".into()])
                .is_err()
        );
        assert!(!c.members.origins.guarded("UOVERFLOW"));
        assert!(
            c.members
                .origins
                .propose(&HashMap::new(), &["U".repeat(256)])
                .is_err()
        );
        assert_eq!(c.share_channel, selected);
    }
    let f = setup().await;
    let state = f.h.state;
    let id = f.id;
    let ctx = &f.ctx;
    let before = f.source();
    let users =
        serde_json::to_value(&state.peek().core.workspaces[&ctx.session.team_id].users).unwrap();
    let invalid = "U".repeat(256);
    let result = renderer::run(
        &f.h._dom,
        run_with(state, id, 17, ctx, "Member", |request, scope| {
            let raw = json!({"results":[row("UVALIDPREFIX","Member Prefix",101),row(&invalid,"Member Invalid",101)]});
            async move { scope.injected(request, |_| async move { Ok(raw) }).await }
        }),
    )
    .await;
    assert!(result.is_err());
    assert_eq!(f.source(), before);
    assert_eq!(
        serde_json::to_value(&state.peek().core.workspaces[&ctx.session.team_id].users).unwrap(),
        users
    );
    assert!(
        !active(&state.peek(), id)
            .unwrap()
            .members
            .origins
            .guarded(&invalid)
    );
    assert!(
        !active(&state.peek(), id)
            .unwrap()
            .members
            .origins
            .guarded("UVALIDPREFIX")
    );
    let full: Vec<_> = (0..4095).map(|n| format!("UFULL{n:04}")).collect();
    {
        let mut current = state;
        let mut s = current.write();
        let c = active_mut(&mut s, id).unwrap();
        let registration = c.members.origins.propose(&HashMap::new(), &full).unwrap();
        c.members.origins.commit(registration);
    }
    let rendered = f.renderer.requests().len();
    let result = renderer::run(
        &f.h._dom,
        run_with(state, id, 17, ctx, "Member", |request, scope| async move {
            scope
                .injected(request, |_| async move {
                    Ok(json!({"results":[row("UBUDGET01","Member Budget",101),row("UBUDGET02","Member Budget Two",101)]}))
                })
                .await
        }),
    )
    .await;
    assert!(result.is_err());
    assert_eq!(f.source(), before);
    assert_eq!(
        serde_json::to_value(&state.peek().core.workspaces[&ctx.session.team_id].users).unwrap(),
        users
    );
    assert_eq!(f.renderer.requests().len(), rendered);
    assert!(
        !active(&state.peek(), id)
            .unwrap()
            .members
            .origins
            .guarded("UBUDGET01")
    );
    assert!(
        !active(&state.peek(), id)
            .unwrap()
            .members
            .origins
            .guarded("UBUDGET02")
    );
    assert!(
        active(&state.peek(), id)
            .unwrap()
            .members
            .origins
            .guarded(full.last().unwrap())
    );
}
