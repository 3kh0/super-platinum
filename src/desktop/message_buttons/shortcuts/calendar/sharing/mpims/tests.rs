use super::test_support::*;
use super::*;
use crate::app_home::channel_fetches::calendar_member_test_support as renderer;
use serde_json::json;
use std::{cell::Cell, collections::VecDeque};
use super_platinum_core::slack::client::RequestBody;

#[tokio::test]
async fn later_raw100_rename_and_second_guarded_rename_reach_actual_intake_posting_and_selection() {
    let mut f = setup().await;
    let state = f.h.state;
    let id = f.id;
    let ctx = f.ctx.clone();
    let generation = f.begin("New Team");
    let before = f.native();
    let baseline = f.renderer.requests().len();
    let (tx1, rx1) = tokio::sync::oneshot::channel();
    let (tx2, rx2) = tokio::sync::oneshot::channel();
    let mut receivers = VecDeque::from([rx1, rx2]);
    let calls = Cell::new(0);
    let ingress = crate::bootstrap::posting_ingress::capture(&state, &ctx.transport, &ctx.session);
    let mut work = Box::pin(read::run_with(
        state,
        id,
        generation,
        &ctx,
        "New Team",
        |request, scope| {
            assert!(scope.admitted());
            let RequestBody::Json(body) = &request.body else {
                panic!("JSON")
            };
            let n = calls.get();
            calls.set(n + 1);
            assert_eq!(body["count"], 100);
            assert_eq!(body["use_display_name"], true);
            assert_eq!(body["fuzz"], 1);
            if n == 0 {
                assert!(body.get("marker").is_none());
            } else {
                assert_eq!(body["marker"], "next");
            }
            let receiver = receivers.pop_front().unwrap();
            async move {
                scope
                    .injected(request, |_| async move { receiver.await.unwrap() })
                    .await
            }
        },
        move |rows| ingress.channels(&state, rows),
    ));
    renderer::pending(&f.h._dom, work.as_mut()).await;
    tx1.send(Ok(json!({"results":vec![json!({"id":"GUNKNOWN1","members":[],"details":{}});99],"next_marker":"next"}))).unwrap();
    renderer::pending(&f.h._dom, work.as_mut()).await;
    assert_eq!(calls.get(), 2);
    assert_eq!(f.native(), before);
    assert_eq!(
        f.renderer.requests().len(),
        baseline,
        "no partial source intake"
    );
    tx2.send(Ok(
        json!({"results":[row(GROUP,"New Team",false),null],"next_marker":{"unused":true}}),
    ))
    .unwrap();
    let outcome = renderer::run_pumped(&mut f.h._dom, work).await;
    assert!(!outcome.failed);
    assert!(outcome.legacy.is_empty());
    let acquired = outcome.acquired.unwrap();
    assert_eq!(acquired.ids, [GROUP]);
    assert_eq!(acquired.source.row(GROUP).unwrap()["name"], "New Team");
    assert_eq!(acquired.source.row(GROUP).unwrap()["updated"], 7);
    assert!(f.renderer.requests().len() > baseline);
    let mut stale: Channel =
        serde_json::from_value(json!({"id":GROUP,"is_mpim":true,"name":"Stale raw slot"})).unwrap();
    stale.extra.insert("is_open".into(), json!(true));
    let mut remote = vec![Destination::Channel(stale)];
    {
        let mut state = state;
        assert!(append(
            &mut state.write(),
            id,
            generation,
            &ctx,
            acquired,
            &mut remote
        ));
    }
    assert_eq!(remote.len(), 1);
    assert_eq!(remote[0].choice().name, "New Team");
    assert!(
        !f.publish(remote.clone(), false)
            .iter()
            .any(|v| v.id == GROUP),
        "intake grants no posting permit"
    );
    f.grant("New Team", &remote);
    assert!(
        active(&state.peek(), id)
            .unwrap()
            .mpims
            .facts
            .as_ref()
            .unwrap()
            .current_for(&state.peek(), &ctx.session.team_id)
    );
    let choices = f.publish(remote, false);
    assert!(
        choices
            .iter()
            .any(|v| v.id == GROUP && v.name == "New Team")
    );
    f.select(GROUP);
    assert_eq!(active(&state.peek(), id).unwrap().share_channel, GROUP);
    // An independent visible cached user removes the retained-selected exemption.
    f.select(PEER);
    assert_eq!(active(&state.peek(), id).unwrap().share_channel, PEER);
    f.begin("Newer Team");
    let (outcome, calls) = f
        .acquire(vec![json!({"mpims":[row(GROUP,"Newer Team",true)]})])
        .await;
    assert_eq!(calls, 1);
    assert!(!outcome.failed);
    assert!(
        outcome.legacy.is_empty(),
        "guarded legacy-decodable row cannot grant raw fallback"
    );
    assert_eq!(
        outcome
            .acquired
            .as_ref()
            .unwrap()
            .source
            .row(GROUP)
            .unwrap()["name"],
        "Newer Team"
    );
    let (remote, failed) = f.append(outcome);
    f.grant("Newer Team", &remote);
    let choices = f.publish(remote, failed);
    assert!(
        choices
            .iter()
            .any(|v| v.id == GROUP && v.name == "Newer Team")
    );
    f.select(GROUP);
    assert_eq!(active(&state.peek(), id).unwrap().share_channel, GROUP);
    let s = state.peek();
    let c = active(&s, id).unwrap();
    assert!(c.mpims.origins.independent(GROUP, "Old Team"));
    assert!(!c.mpims.origins.independent(GROUP, "New Team"));
    assert!(!c.mpims.origins.independent(GROUP, "Newer Team"));
    assert_eq!(c.notice.as_deref(), Some("Unrelated preference warning"));
    assert_eq!(super::super::resolve(&s, id).as_deref(), Some(GROUP));
    drop(s);
    // A new, failed refresh of a previously guarded legacy-decodable ID cannot
    // promote its raw first-page rename through the independent legacy route.
    f.begin("Team");
    f.select(PEER);
    assert_eq!(active(&state.peek(), id).unwrap().share_channel, PEER);
    f.begin("Failed Team");
    let before = f.native();
    let (outcome, calls) = f
        .acquire_results(vec![
            Ok(json!({"mpims":[row(GROUP,"Failed Team Raw",true)],"next_marker":"next"})),
            Err(Error::Transport("guarded refresh successor failed".into())),
        ])
        .await;
    assert_eq!(calls, 2);
    assert!(outcome.failed);
    assert!(outcome.acquired.is_none());
    assert!(outcome.legacy.is_empty());
    assert_eq!(f.native(), before);
    let (remote, failed) = f.append(outcome);
    let choices = f.publish(remote, failed);
    assert!(!choices.iter().any(|v| v.id == GROUP));
    f.select(GROUP);
    assert_eq!(active(&state.peek(), id).unwrap().share_channel, PEER);
}

#[tokio::test]
async fn independent_legacy_survives_late_failure_and_retired_issued_429_without_partial_intake() {
    for case in 0..3 {
        let mut f = setup().await;
        let mut state = f.h.state;
        let ctx = f.ctx.clone();
        let id = f.id;
        let generation = f.begin("New Team");
        if case == 2 {
            state
                .write()
                .core
                .workspaces
                .get_mut(&ctx.session.team_id)
                .unwrap()
                .channels
                .get_mut(GROUP)
                .unwrap()
                .name = Some("Unrelated cache drift".into());
        }
        let before = f.native();
        let rendered = f.renderer.requests().len();
        let callbacks = Cell::new(0);
        let calls = Cell::new(0);
        let (send, receive) = tokio::sync::oneshot::channel();
        let mut receive = Some(receive);
        let ingress =
            crate::bootstrap::posting_ingress::capture(&state, &ctx.transport, &ctx.session);
        let mut work = Box::pin(read::run_with(
            state,
            id,
            generation,
            &ctx,
            "New Team",
            |request, scope| {
                let n = calls.get();
                calls.set(n + 1);
                let held = if n == 1 { receive.take() } else { None };
                async move {
                    scope.injected(request,|_|async move {
                if let Some(held)=held {return held.await.unwrap();}
                Ok(json!({"mpims":[row("GLEGACY01","New Team Legacy",true)],"results":[row(GROUP,"New Team",false)],"next_marker":"next"}))
            }).await
                }
            },
            |rows| {
                callbacks.set(callbacks.get() + 1);
                assert_eq!(rows.len(), 1);
                ingress.channels(&state, rows);
            },
        ));
        if case < 2 {
            renderer::pending(&f.h._dom, work.as_mut()).await;
            assert_eq!(calls.get(), 2);
            if case == 1 {
                let mut retiring_state = state;
                active_mut(&mut retiring_state.write(), id)
                    .unwrap()
                    .share_generation += 1;
            }
            send.send(Err(if case == 1 {
                Error::RateLimited {
                    retry_after_secs: Some(37),
                }
            } else {
                Error::Transport("later page failed".into())
            }))
            .unwrap();
        }
        let outcome = renderer::run_pumped(&mut f.h._dom, work).await;
        assert!(outcome.acquired.is_none());
        assert_eq!(callbacks.get(), 1);
        assert_eq!(f.native(), before);
        assert_eq!(f.renderer.requests().len(), rendered);
        if case == 1 {
            assert_eq!(
                outcome.legacy.len(),
                1,
                "independent first response stays distinct; parent rejects retired publication"
            );
            let choices = active(&state.peek(), id).unwrap().share_choices.clone();
            super::super::publish(
                &mut state.write(),
                id,
                generation,
                &ctx,
                "New Team",
                outcome
                    .legacy
                    .iter()
                    .cloned()
                    .map(Destination::Channel)
                    .collect(),
                true,
            );
            assert_eq!(active(&state.peek(), id).unwrap().share_choices, choices);
            let s = state.peek();
            let until = s.message_buttons.shortcuts.cooldowns
                [&(ctx.epoch, ctx.session.team_id.clone())]
                .unwrap();
            assert!(until.saturating_duration_since(Instant::now()) > Duration::from_secs(30));
        } else {
            assert_eq!(outcome.legacy.len(), 1);
            assert_eq!(outcome.legacy[0].id, "GLEGACY01");
        }
        if case == 2 {
            assert_eq!(
                calls.get(),
                1,
                "independent first page survives unavailable canonical proof"
            );
        }
        assert!(
            !active(&state.peek(), id)
                .unwrap()
                .mpims
                .origins
                .guarded(GROUP)
        );
        assert_eq!(
            active(&state.peek(), id).unwrap().notice.as_deref(),
            Some("Unrelated preference warning")
        );
    }
}

#[tokio::test]
async fn all_id_guard_append_and_precap_currentness_refuse_stale_selection_without_cache_fallback()
{
    let mut f = setup().await;
    let mut state = f.h.state;
    let ctx = f.ctx.clone();
    let id = f.id;
    let generation = f.begin("Group");
    let (outcome, _) = f
        .acquire(vec![
            json!({"results":[row(GROUP,"Group A0000",false),row(SECOND,"Group A0001",false)]}),
        ])
        .await;
    assert!(!outcome.failed);
    let acquired = outcome.acquired.unwrap();
    assert_eq!(acquired.ids, [GROUP, SECOND]);
    let mut remote = Vec::new();
    {
        let mut s = state.write();
        let old = active(&s, id).unwrap().mpims.origins.clone();
        active_mut(&mut s, id).unwrap().mpims.origins = old.without(SECOND);
        assert!(!append(
            &mut s,
            id,
            generation,
            &ctx,
            Acquired {
                source: acquired.source.clone(),
                ids: acquired.ids.clone()
            },
            &mut remote
        ));
        assert!(remote.is_empty());
        assert!(active(&s, id).unwrap().mpims.hits.is_empty());
        assert!(active(&s, id).unwrap().mpims.facts.is_none());
        active_mut(&mut s, id).unwrap().mpims.origins = old;
        assert!(append(&mut s, id, generation, &ctx, acquired, &mut remote));
    }
    let mut raw = vec![
        json!({"id":"CINIT0001","is_channel":true,"name":"warm-harbor","is_member":true}),
        json!({"id":GROUP,"is_mpim":true,"is_private":true,"name":"Group A0000","is_member":true}),
        json!({"id":SECOND,"is_mpim":true,"is_private":true,"name":"Group A0001","is_member":true}),
    ];
    posting::test_acquire(&mut state.write(), id, raw.clone());
    let choices = f.publish(remote.clone(), false);
    assert!(choices.iter().any(|v| v.id == GROUP));
    let selected = active(&state.peek(), id).unwrap().share_channel.clone();
    {
        let mut s = state.write();
        let w = s.core.workspaces.get_mut(&ctx.session.team_id).unwrap();
        for n in 0..150 {
            let value = json!({"id":format!("GLEGACY{n:04}"),"is_channel":true,"is_member":true,"name":format!("Group B{n:04}")});
            let channel: Channel = serde_json::from_value(value.clone()).unwrap();
            w.channels.insert(channel.id.clone(), channel);
            raw.push(value);
        }
    }
    posting::test_acquire(&mut state.write(), id, raw);
    let legacy = load::share_choices(&state.peek(), &ctx, "Group", &selected);
    let admitted = super::super::members::choices(&state.peek(), id, &ctx, "Group", &selected);
    assert_eq!(admitted.len(), 100);
    assert!(legacy.iter().any(|v| v.id == GROUP));
    assert!(!admitted.iter().any(|v| v.id == GROUP || v.id == SECOND));
    assert!(!legacy.iter().any(|v| v.id == "GLEGACY0098"));
    assert!(admitted.iter().any(|v| v.id == "GLEGACY0098"));
    f.select(GROUP);
    assert_eq!(active(&state.peek(), id).unwrap().share_channel, selected);
    // Registry and whole-bundle ID refusal precede any shared acceptance/renderer work.
    let mut f = setup().await;
    let mut state = f.h.state;
    let ctx = f.ctx.clone();
    let id = f.id;
    f.begin("Group");
    let before = f.native();
    let rendered = f.renderer.requests().len();
    let invalid = "G".repeat(256);
    let(outcome,_)=f.acquire(vec![json!({"results":[row(GROUP,"Group Prefix",false),row(&invalid,"Group Invalid",false)]})]).await;
    assert!(outcome.failed);
    assert!(outcome.acquired.is_none());
    assert_eq!(f.native(), before);
    assert!(
        !active(&state.peek(), id)
            .unwrap()
            .mpims
            .origins
            .guarded(GROUP)
    );
    let ids: Vec<_> = (0..4095).map(|n| format!("GREGISTRY{n:04}")).collect();
    {
        let mut s = state.write();
        let registration = active(&s, id)
            .unwrap()
            .mpims
            .origins
            .propose(&s, &ctx, &ids)
            .unwrap();
        active_mut(&mut s, id)
            .unwrap()
            .mpims
            .origins
            .commit(registration);
    }
    let(outcome,_)=f.acquire(vec![json!({"results":[row(GROUP,"Group Budget",false),row(SECOND,"Group Budget Two",false)]})]).await;
    assert!(outcome.failed);
    assert!(outcome.acquired.is_none());
    assert_eq!(f.native(), before);
    assert_eq!(f.renderer.requests().len(), rendered);
    let s = state.peek();
    let origins = &active(&s, id).unwrap().mpims.origins;
    assert!(origins.guarded(ids.last().unwrap()));
    assert!(!origins.guarded(GROUP));
    assert!(!origins.guarded(SECOND));
    drop(s);
    let mut f = setup().await;
    let mut state = f.h.state;
    let ctx = f.ctx.clone();
    let id = f.id;
    f.begin("Old Team");
    let (outcome, _) = f
        .acquire(vec![json!({"results":[row(GROUP,"Old Team",false)]})])
        .await;
    assert!(!outcome.failed);
    let (remote, failed) = f.append(outcome);
    f.grant("Old Team", &remote);
    assert!(f.publish(remote, failed).iter().any(|v| v.id == GROUP));
    {
        let mut s = state.write();
        let independent: Channel = serde_json::from_value(
            json!({"id":"GINDEP001","is_channel":true,"name":"Unrelated independent cache"}),
        )
        .unwrap();
        s.core
            .workspaces
            .get_mut(&ctx.session.team_id)
            .unwrap()
            .channels
            .insert(independent.id.clone(), independent);
        let c = active(&s, id).unwrap();
        assert!(c.mpims.origins.independent(GROUP, "Old Team"));
        assert!(
            !c.mpims
                .facts
                .as_ref()
                .unwrap()
                .current_for(&s, &ctx.session.team_id)
        );
    }
    // Original same-label cache route survives without current canonical proof,
    // but the separately acquired current posting policy remains mandatory.
    assert!(
        super::super::members::choices(&state.peek(), id, &ctx, "Old Team", PEER)
            .iter()
            .any(|v| v.id == GROUP)
    );
    active_mut(&mut state.write(), id)
        .unwrap()
        .posting
        .invalidate();
    assert!(
        !super::super::members::choices(&state.peek(), id, &ctx, "Old Team", PEER)
            .iter()
            .any(|v| v.id == GROUP)
    );
    let mut origins = origins::Origins::default();
    let ids = vec!["GNOORIGIN".to_owned()];
    origins.commit(origins.propose(&state.peek(), &ctx, &ids).unwrap());
    {
        let mut s = state.write();
        let later: Channel = serde_json::from_value(
            json!({"id":"GNOORIGIN","is_channel":true,"name":"Later label"}),
        )
        .unwrap();
        s.core
            .workspaces
            .get_mut(&ctx.session.team_id)
            .unwrap()
            .channels
            .insert(later.id.clone(), later);
    }
    origins.commit(origins.propose(&state.peek(), &ctx, &ids).unwrap());
    assert!(origins.guarded("GNOORIGIN"));
    assert!(!origins.independent("GNOORIGIN", "Later label"));
}
