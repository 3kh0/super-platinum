use super::super::test_support::*;
use super::super::*;
use super::matching;
use crate::app_home::channel_fetches::calendar_member_test_support as renderer;
use serde_json::json;

fn metadata(f: &Fixture) {
    let mut state = f.h.state;
    posting::test_acquire(
        &mut state.write(),
        f.id,
        vec![
            json!({"id":"CINIT0001","is_channel":true,"is_member":true,"name":"warm-harbor"}),
            json!({"id":GROUP,"is_channel":true,"is_mpim":true,"is_private":true,"is_member":true,"name":"Planning room"}),
        ],
    );
    // The literal response went through the real BootRead producer. Remove its
    // test convenience witnesses; production refresh must acquire its own.
    active_mut(&mut state.write(), f.id)
        .unwrap()
        .posting
        .invalidate();
}
async fn acquired(f: &mut Fixture, query: &str) -> Vec<Destination> {
    f.begin(query);
    let (outcome, calls) = f
        .acquire(vec![json!({"results":[row(GROUP,"Planning room",false)]})])
        .await;
    assert_eq!(calls, 1);
    assert!(!outcome.failed);
    assert!(outcome.acquired.is_some());
    let (remote, failed) = f.append(outcome);
    assert!(!failed);
    remote
}
fn retire_member(f: &Fixture) {
    let mut state = f.h.state;
    state
        .write()
        .core
        .workspaces
        .get_mut(&f.ctx.session.team_id)
        .unwrap()
        .users
        .get_mut(PEER)
        .unwrap()
        .profile
        .as_mut()
        .unwrap()
        .display_name = Some("unrepresented-member-change".into());
    let s = state.peek();
    let c = active(&s, f.id).unwrap();
    assert!(
        c.mpims
            .facts
            .as_ref()
            .unwrap()
            .current_for(&s, &f.ctx.session.team_id),
        "channel proof must remain current"
    );
    assert!(
        !c.mpims.participants.as_ref().unwrap().current(&s),
        "only participant proof retired"
    );
}
fn query_generation(f: &Fixture) -> u64 {
    active(&f.h.state.peek(), f.id).unwrap().share_generation
}

#[tokio::test]
async fn actual_participant_match_without_label_match_obtains_real_posting_witness_and_selects_current_group()
 {
    let mut f = setup_with_peers(&[(PEER, "target", None), ("UOTHER001", "zzzzzz", None)]).await;
    let state = f.h.state;
    let id = f.id;
    let remote = acquired(&mut f, "target").await;
    {
        let s = state.peek();
        let c = active(&s, id).unwrap();
        let frame = c
            .mpims
            .participants
            .as_ref()
            .expect("actual current complete member-source match");
        assert!(frame.current(&s));
        assert!(frame.contains(GROUP));
        assert!(
            !s.core.workspaces[&f.ctx.session.team_id].channels[GROUP]
                .name
                .as_ref()
                .unwrap()
                .contains("target")
        );
    }
    assert!(
        posting::test_candidates(&state.peek(), id, &remote)
            .iter()
            .any(|v| v == GROUP)
    );
    assert!(
        !f.publish(remote.clone(), false)
            .iter()
            .any(|v| v.id == GROUP),
        "source match grants no permission"
    );
    metadata(&f);
    assert!(
        !f.publish(remote.clone(), false)
            .iter()
            .any(|v| v.id == GROUP),
        "cached raw metadata alone is not a query witness"
    );
    let generation = query_generation(&f);
    assert!(
        renderer::run_pumped(
            &mut f.h._dom,
            posting::refresh(state, id, generation, &remote)
        )
        .await
    );
    let choices = f.publish(remote, false);
    assert!(
        choices
            .iter()
            .any(|v| v.id == GROUP && v.name == "Planning room")
    );
    f.select(GROUP);
    let s = state.peek();
    let c = active(&s, id).unwrap();
    assert_eq!(c.share_channel, GROUP);
    let Some(Destination::Channel(selected)) = &c.share_selected else {
        panic!("current canonical remote selection")
    };
    assert_eq!(selected.name.as_deref(), Some("Planning room"));
    assert_eq!(super::super::super::resolve(&s, id).as_deref(), Some(GROUP));
    assert_eq!(c.notice.as_deref(), Some("Unrelated preference warning"));
}

#[tokio::test]
async fn complete_peer_sum_and_distinct_permission_or_queued_selection_retirement_never_use_raw_fallback()
 {
    // Independent aggregate rejection: exact anna contributes0; FOUR other
    // canonical peers contribute1 each. Raw outer members omit those four.
    let mut f = setup_with_peers(&[
        (PEER, "anna", None),
        ("UOTHER001", "zzzzzz", None),
        ("UFINITE01", "z anna", Some("z anna")),
        ("UFINITE02", "y anna", Some("y anna")),
        ("UFINITE03", "w anna", Some("w anna")),
        ("UFINITE04", "v anna", Some("v anna")),
    ])
    .await;
    let remote = acquired(&mut f, "anna").await;
    {
        let s = f.h.state.peek();
        let c = active(&s, f.id).unwrap();
        let source = c.mpims.facts.as_ref().unwrap().row(GROUP).unwrap();
        assert_eq!(
            source["members"].as_array().unwrap().len(),
            7,
            "score canonical membership after union"
        );
        assert!(
            source["members"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == "UFINITE04")
        );
        let member_source = crate::directory_member_fetches::search::snapshot(
            &s,
            &f.ctx.session.team_id,
            "aggregate-score-diagnostic",
        )
        .unwrap();
        assert!(member_source.current(&s));
        let peers: BTreeMap<_, _> = member_source
            .rows()
            .iter()
            .map(|v| {
                (
                    v.source["id"].as_str().unwrap().to_owned(),
                    super::peer(&v.source).unwrap(),
                )
            })
            .collect();
        assert!(
            source["members"]
                .as_array()
                .unwrap()
                .iter()
                .all(|v| peers.contains_key(v.as_str().unwrap())),
            "complete actual source inventory"
        );
        assert_eq!(peers[&f.ctx.session.user_id]["is_self"], true);
        let matcher = matching::FuzzyMatcher::new("anna", matching::FuzzyOptions::default());
        assert_eq!(
            matcher.score(peers[PEER]["_display_name_normalized_lc"].as_str().unwrap()),
            Some(0)
        );
        for (id, name) in [
            ("UFINITE01", "z anna"),
            ("UFINITE02", "y anna"),
            ("UFINITE03", "w anna"),
            ("UFINITE04", "v anna"),
        ] {
            assert_eq!(peers[id]["_real_name_normalized_lc"], name);
            assert_eq!(peers[id]["_display_name_normalized_lc"], name);
            assert_ne!(peers[id]["is_self"], true);
            assert_eq!(matcher.score(name), Some(1));
        }
        let mut options = matching::Options::workflow("anna", "anna", "anna", false);
        options.match_email = true;
        options.email_forward_only = true;
        let queries = vec!["anna".to_owned()];
        let remote_ids = BTreeSet::new();
        assert!(
            matching::filter_mpims(
                &[source.clone()],
                &peers,
                &options,
                &queries,
                &remote_ids,
                |_| true,
                |_| None
            )
            .is_empty()
        );
        let mut without_one = source.clone();
        without_one["members"]
            .as_array_mut()
            .unwrap()
            .retain(|v| v != "UFINITE04");
        let rows = [without_one];
        let accepted = matching::filter_mpims(
            &rows,
            &peers,
            &options,
            &queries,
            &remote_ids,
            |_| true,
            |_| None,
        );
        assert_eq!(
            accepted.len(),
            1,
            "same actual inputs accept when aggregate is3"
        );
        assert_eq!(accepted[0].result.score, 3);
        assert!(
            c.mpims.participants.is_none(),
            "all finite scores sum to4, above3"
        );
        assert!(
            !posting::test_candidates(&s, f.id, &remote)
                .iter()
                .any(|v| v == GROUP)
        );
    }
    assert!(!f.publish(remote, false).iter().any(|v| v.id == GROUP));

    // Separate ACCEPTED input reaches a held real posting-acquisition boundary.
    // Its current raw metadata remains valid; private participant proof retires.
    let mut f = setup_with_peers(&[(PEER, "target", None), ("UOTHER001", "zzzzzz", None)]).await;
    let state = f.h.state;
    let id = f.id;
    let remote = acquired(&mut f, "target").await;
    assert!(
        posting::test_candidates(&state.peek(), id, &remote)
            .iter()
            .any(|v| v == GROUP)
    );
    metadata(&f);
    let generation = query_generation(&f);
    let cache = f.native();
    let (selected, title, description) = {
        let s = state.peek();
        let c = active(&s, id).unwrap();
        (
            c.share_channel.clone(),
            c.title.clone(),
            c.description.clone(),
        )
    };
    let (hold, release) = posting::permission_test_support::hold(id, generation, f.ctx.clone());
    let mut permission = Box::pin(posting::refresh(state, id, generation, &remote));
    renderer::pending(&f.h._dom, permission.as_mut()).await;
    assert!(hold.entered());
    retire_member(&f);
    release.send(()).unwrap();
    assert!(
        renderer::run_pumped(&mut f.h._dom, permission).await,
        "actual valid metadata acquisition still completes"
    );
    drop(hold);
    assert!(
        posting::allowed(
            &state.peek(),
            &f.ctx,
            &state.peek().core.workspaces[&f.ctx.session.team_id].channels[GROUP]
        ),
        "permission current while participant proof is stale"
    );
    assert!(!f.publish(remote, false).iter().any(|v| v.id == GROUP));
    f.select(GROUP);
    {
        let s = state.peek();
        let c = active(&s, id).unwrap();
        assert_eq!(c.share_channel, selected);
        assert_eq!(c.title, title);
        assert_eq!(c.description, description);
        assert!(c.share_remote.iter().all(|v| v.id() != GROUP));
    }
    assert_eq!(f.native(), cache);

    // Separate publication succeeds before its queued selection becomes stale.
    let mut f = setup_with_peers(&[(PEER, "target", None), ("UOTHER001", "zzzzzz", None)]).await;
    let state = f.h.state;
    let id = f.id;
    let remote = acquired(&mut f, "target").await;
    metadata(&f);
    let generation = query_generation(&f);
    assert!(
        renderer::run_pumped(
            &mut f.h._dom,
            posting::refresh(state, id, generation, &remote)
        )
        .await
    );
    let choices = f.publish(remote.clone(), false);
    assert!(choices.iter().any(|v| v.id == GROUP));
    let queued = choices.iter().find(|v| v.id == GROUP).unwrap().id.clone();
    let cache = f.native();
    let (selected, title, description) = {
        let s = state.peek();
        let c = active(&s, id).unwrap();
        (
            c.share_channel.clone(),
            c.title.clone(),
            c.description.clone(),
        )
    };
    retire_member(&f);
    assert!(
        active(&state.peek(), id)
            .unwrap()
            .share_choices
            .iter()
            .any(|v| v.id == queued),
        "queued old choice still present"
    );
    f.select(&queued);
    {
        let s = state.peek();
        let c = active(&s, id).unwrap();
        assert_eq!(c.share_channel, selected);
        assert!(c.share_selected.is_none());
        assert_eq!(c.title, title);
        assert_eq!(c.description, description);
    }
    assert_eq!(f.native(), cache);
    assert!(!f.publish(remote, false).iter().any(|v| v.id == GROUP));
}
