use super::*;

#[test]
fn observed_ranking_reads_use_actual_builders_and_closed_defaults() {
    use super_platinum_core::block_actions::home::workflow::directory_observations as observed;
    let (mut s, origin) = setup(Mode::Accepted);
    let context = &origin.owner.ctx;
    let model = observed::model_request(&context.client, &context.session);
    let Reply::Ready(raw) = begin(&mut s, &origin, model.clone(), false).unwrap() else {
        panic!("read unexpectedly held")
    };
    assert!(observed::decode_model(&raw).unwrap().is_none());
    assert!(begin(&mut s, &origin, model, true).is_err());
    let request = observed::offline_request(&context.client, &context.session);
    let Reply::Ready(raw) = begin(&mut s, &origin, request, false).unwrap() else {
        panic!("read unexpectedly held")
    };
    let offline = observed::Offline::decode(&raw).unwrap();
    assert_eq!(offline.entity_ctr(observed::Entity::Member), 0.132);
    assert_eq!(offline.entity_ctr(observed::Entity::Im), 0.179);
    assert!(s.app_home.workflow_attempts.is_empty());
    assert!(!s.app_home.workflow_fixture.as_ref().unwrap().started);
}

pub(super) fn setup(mode: Mode) -> (ShellState, Origin) {
    let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
    install(&mut s, mode);
    let origin = s.app_home.workflow_fixture.as_ref().unwrap().origin.clone();
    assert!(origin.current(&s));
    (s, origin)
}
fn request(s: &mut ShellState, origin: &Origin) -> (Preflight, PreparedRequest) {
    let view = s
        .app_home
        .surface
        .as_ref()
        .unwrap()
        .view
        .as_ref()
        .unwrap()
        .clone();
    let req = origin
        .workflow
        .precheck(
            &origin.owner.ctx.client,
            &origin.owner.ctx.session,
            &view,
            "precheck",
        )
        .unwrap();
    let Reply::Ready(raw) = begin(s, origin, req, true).unwrap() else {
        panic!("precheck unexpectedly held")
    };
    let TripResult::Review(mut review) = origin.workflow.response(raw).unwrap() else {
        panic!("expected actual preflight")
    };
    review.set_custom("title", "Release 7".into()).unwrap();
    review.set_custom("person", "UWORKADA1".into()).unwrap();
    review
        .set_custom("destination", "DWORKFLOW".into())
        .unwrap();
    let req = review
        .prepare_start(
            &origin.owner.ctx.client,
            &origin.owner.ctx.session,
            &view,
            "start",
        )
        .unwrap();
    (review, req)
}
fn sent(s: &mut ShellState, origin: &Origin) -> u64 {
    let id = next();
    s.app_home.workflow_attempts.push(Attempt {
        id,
        origin: origin.clone(),
        phase: Phase::Sent,
        cancelled: false,
        lease: None,
        token: Some("start".into()),
    });
    id
}
#[test]
fn actual_request_builders_decode_preflight_and_exact_selected_ids_once() {
    let (mut s, origin) = setup(Mode::Accepted);
    let (review, req) = request(&mut s, &origin);
    let Reply::Ready(raw) = begin(&mut s, &origin, req.clone(), true).unwrap() else {
        panic!("unexpected hold")
    };
    assert!(matches!(
        origin.workflow.response(raw),
        Ok(TripResult::Accepted { .. })
    ));
    assert!(begin(&mut s, &origin, req, true).is_err());
    assert_eq!(review.value("destination", true), Some("DWORKFLOW"));
    let ledger = &s.app_home.workflow_fixture.as_ref().unwrap().ledger;
    assert!(ledger.iter().all(|row| row["body"].get("token").is_none()));
}
#[test]
fn held_result_is_retained_after_owner_replacement_without_updating_new_home() {
    let (mut s, origin) = setup(Mode::Held);
    let (review, req) = request(&mut s, &origin);
    let id = sent(&mut s, &origin);
    let Reply::Held(mut receiver) = begin(&mut s, &origin, req, true).unwrap() else {
        panic!("expected hold")
    };
    assert!(matches!(
        receiver.try_recv(),
        Err(tokio::sync::oneshot::error::TryRecvError::Empty)
    ));
    s.channel_generation += 1;
    s.app_home.surface.as_mut().unwrap().notice = Some("Replacement retained".into());
    assert!(release(&mut s));
    let result = receiver.try_recv().unwrap();
    assert!(settle(&mut s, id, &origin, &Operation::Start(review), None, result).is_none());
    assert_eq!(
        s.app_home.surface.as_ref().unwrap().notice.as_deref(),
        Some("Replacement retained")
    );
    assert!(!release(&mut s));
    assert!(s.app_home.workflow_fixture.as_ref().unwrap().started);
}
#[test]
fn lost_result_keeps_actual_uncertain_attempt_and_does_not_replay() {
    let (mut s, origin) = setup(Mode::Unconfirmed);
    let (review, req) = request(&mut s, &origin);
    let id = sent(&mut s, &origin);
    let result = match begin(&mut s, &origin, req.clone(), true) {
        Err(error) => Err(error),
        Ok(_) => panic!("expected lost response"),
    };
    settle(&mut s, id, &origin, &Operation::Start(review), None, result);
    assert!(
        s.app_home
            .workflow_attempts
            .iter()
            .any(|a| a.id == id && a.phase == Phase::Uncertain)
    );
    assert!(begin(&mut s, &origin, req, true).is_err());
}
#[test]
fn actual_rate_limit_settlement_retains_cooldown_without_uncertain_delivery() {
    let (mut s, origin) = setup(Mode::RateLimited);
    let (review, req) = request(&mut s, &origin);
    let id = sent(&mut s, &origin);
    let result = match begin(&mut s, &origin, req, true) {
        Err(error) => Err(error),
        Ok(_) => panic!("expected 429"),
    };
    assert!(matches!(
        &result,
        Err(Error::RateLimited {
            retry_after_secs: Some(2)
        })
    ));
    settle(&mut s, id, &origin, &Operation::Start(review), None, result);
    assert!(!s.app_home.workflow_attempts.iter().any(|a| a.id == id));
    assert!(cooling(&s, &origin.owner.ctx));
    assert!(!s.app_home.workflow_fixture.as_ref().unwrap().started);
}
#[test]
fn foreign_request_or_retired_owner_cannot_enter_closed_executor() {
    let (mut s, origin) = setup(Mode::Accepted);
    let (_, req) = request(&mut s, &origin);
    let count = s.app_home.workflow_fixture.as_ref().unwrap().ledger.len();
    let mut foreign = req.clone();
    foreign.url = "https://slack.com/api/workflows.triggers.trip".into();
    assert!(begin(&mut s, &origin, foreign, true).is_err());
    s.channel_generation += 1;
    assert!(begin(&mut s, &origin, req, true).is_err());
    assert_eq!(
        s.app_home.workflow_fixture.as_ref().unwrap().ledger.len(),
        count
    );
}

#[test]
fn meeting_reads_use_actual_closed_builders_and_preserve_unavailable_status() {
    use super_platinum_core::block_actions::home::workflow::meeting_events as meeting;
    for mode in [
        MeetingMode::Known,
        MeetingMode::AllDay,
        MeetingMode::TitleOnly,
        MeetingMode::StatusUnavailable,
        MeetingMode::Disconnected,
    ] {
        let mut s = ShellState::fixture(crate::media::MediaRegistry::default());
        install_meeting(&mut s, mode);
        let origin = s.app_home.workflow_fixture.as_ref().unwrap().origin.clone();
        let ctx = &origin.owner.ctx;
        let request = meeting::request::user_status(&ctx.client, &ctx.session);
        let status = begin(&mut s, &origin, request, false);
        if mode == MeetingMode::StatusUnavailable {
            assert!(matches!(status, Err(Error::Api(_))));
            continue;
        }
        let Reply::Ready(raw) = status.unwrap() else {
            panic!("status read held")
        };
        let status = meeting::CalendarStatus::decode(raw).unwrap();
        if mode == MeetingMode::Disconnected {
            assert!(!status.connected());
            continue;
        }
        let req = meeting::request::event_by_link(&ctx.client, &ctx.session, &status, "LWORKFLOW")
            .unwrap();
        let Reply::Ready(raw) = begin(&mut s, &origin, req.clone(), false).unwrap() else {
            panic!("event read held")
        };
        let event = meeting::decode_event("LWORKFLOW", &raw).unwrap();
        assert_eq!(
            event.format_input().unwrap().all_day,
            mode == MeetingMode::AllDay
        );
        assert_eq!(
            event.format_input().unwrap().start.is_none(),
            mode == MeetingMode::TitleOnly
        );
        assert!(begin(&mut s, &origin, req, true).is_err());
        let wrong =
            meeting::request::event_by_link(&ctx.client, &ctx.session, &status, "OTHER").unwrap();
        assert!(begin(&mut s, &origin, wrong, false).is_err());
        assert!(!s.app_home.workflow_fixture.as_ref().unwrap().started);
    }
}

#[test]
fn native_names_use_actual_owned_boot_and_complete_preference_observations() {
    use super_platinum_core::{
        block_actions::name_policy as policy,
        state::posting_metadata::authority::{Authority, BootRead},
    };
    let (mut s, origin) = setup(Mode::Accepted);
    let ctx = &origin.owner.ctx;
    let read = BootRead::begin(&mut s.core, &ctx.transport, &ctx.session).unwrap();
    let request = read.request(&s.core).unwrap();
    let Reply::Ready(raw) = begin(&mut s, &origin, request, false).unwrap() else {
        panic!("boot held")
    };
    assert!(read.apply(&mut s.core, raw, false).unwrap());
    let authority = Authority::capture(&s.core, &ctx.session.team_id).unwrap();
    let (membership, enterprise) = authority
        .name_policy_membership(&s.core, policy::OrdinaryClientV2)
        .unwrap();
    assert!(enterprise.is_none());
    let Reply::Ready(gantry) = begin(
        &mut s,
        &origin,
        super_platinum_core::block_actions::shortcuts::grid::discovery(&ctx.client, &ctx.session),
        false,
    )
    .unwrap() else {
        panic!("gantry membership held")
    };
    super_platinum_core::block_actions::shortcuts::grid::Membership::decode(
        gantry.clone(),
        &ctx.session,
    )
    .unwrap();
    assert_eq!(gantry["self"]["id"], ctx.session.user_id);
    assert_eq!(gantry["default_workspace"], ctx.session.team_id);
    assert_eq!(
        gantry["workspaces"].as_array().unwrap().len(),
        membership.workspace_ids().count()
    );
    let Reply::Ready(raw) = begin(
        &mut s,
        &origin,
        policy::request::user(&ctx.client, &ctx.session),
        false,
    )
    .unwrap() else {
        panic!("prefs held")
    };
    let policy = policy::Snapshot::new(
        policy::OrdinaryClientV2,
        membership,
        policy::UserPrefs::decode(&raw).unwrap(),
        None,
    )
    .unwrap();
    assert!(!policy.should_display_real_names().unwrap());
    assert!(policy.missing_team_preferences().is_empty());
    assert!(!s.app_home.workflow_fixture.as_ref().unwrap().started);
    assert!(authority.current(&s.core));
}
