use super::*;

pub(super) fn setup() -> (ShellState, Visit) {
    let (mut shell, visit) = super::super::tests::shell();
    shell.core.active_account = Some("00000000-0000-4000-8000-000000000081".into());
    let account = shell.core.active_account.clone();
    shell
        .app_home
        .about
        .management
        .surface
        .as_mut()
        .unwrap()
        .account = account.clone();
    for d in &mut shell.app_home.about.management.documents {
        d.account = account.clone();
    }
    // Keep the real About bot owner while using a valid directory member ID.
    let owner = shell.app_home.about.surface.as_ref().unwrap().owner.clone();
    let w = shell
        .core
        .workspaces
        .get_mut(&owner.ctx.session.team_id)
        .unwrap();
    let mut bot = w.users.remove(&owner.bot).unwrap();
    bot.id = "UACTBOT01".into();
    w.users.insert(bot.id.clone(), bot);
    w.channels.get_mut(&owner.channel).unwrap().user = Some("UACTBOT01".into());
    shell.app_home.about.surface.as_mut().unwrap().owner.bot = "UACTBOT01".into();
    let updated = shell.app_home.about.surface.as_ref().unwrap().owner.clone();
    shell
        .app_home
        .about
        .management
        .surface
        .as_mut()
        .unwrap()
        .owner = updated.into();
    shell.signed_in = true;
    shell.loading = false;
    shell.connection.routable = false;
    shell.connection.routed_at = Some(Instant::now());
    (shell, visit)
}
fn show(shell: &mut ShellState, visit: Visit, active: bool) {
    if let Some(d) = document_mut(shell, visit) {
        d.activity.active = active;
        d.activity.member_generation = next();
        d.activity.member_pending = None;
        d.activity.member_results = None;
    }
}

#[test]
fn picker_failure_is_scoped_preserves_other_feedback_and_retires_with_query() {
    let (mut shell, visit) = setup();
    shell.show_toast("Unrelated upload status");
    let query = Query::capture(&mut shell, visit).unwrap();
    lookup::record_failure(&mut shell, &query, Error::Offline("private-url".into()));
    let notice = &document(&shell, visit).unwrap().activity.member_notice;
    assert_eq!(
        notice.as_deref(),
        Some("People are unavailable while offline. Try again when connected.")
    );
    assert_eq!(
        shell.toast.as_ref().unwrap().text,
        "Unrelated upload status"
    );
    assert!(matches!(
        *query.failure_receipt.lock().unwrap(),
        Some(Error::Offline(_))
    ));
    lookup::record_failure(&mut shell, &query, Error::Api("private-api-details".into()));
    assert_eq!(
        document(&shell, visit)
            .unwrap()
            .activity
            .member_notice
            .as_deref(),
        Some("People could not be loaded. Try again.")
    );
    assert!(matches!(
        *query.failure_receipt.lock().unwrap(),
        Some(Error::Api(_))
    ));
    edit(&mut shell, visit, "new query".into());
    let before = document(&shell, visit)
        .unwrap()
        .activity
        .member_notice
        .clone();
    lookup::record_failure(&mut shell, &query, Error::Offline("late".into()));
    assert_eq!(
        document(&shell, visit).unwrap().activity.member_notice,
        before
    );
    assert!(matches!(
        *query.failure_receipt.lock().unwrap(),
        Some(Error::Api(_))
    ));
    assert_eq!(
        shell.toast.as_ref().unwrap().text,
        "Unrelated upload status"
    );
}

#[test]
fn query_retains_whole_activity_filter_and_retires_on_same_text_input_or_reentry() {
    let (mut shell, visit) = setup();
    edit(&mut shell, visit, "+".into());
    let query = Query::capture(&mut shell, visit).unwrap();
    assert!(query.current(&shell));
    assert!(query.path() == SearchPath::Local);
    assert_eq!(query.member.text(SearchPath::Remote), "+");
    assert_eq!(query.text(), "");
    edit(&mut shell, visit, "+".into());
    assert!(!query.current(&shell));
    let query = Query::capture(&mut shell, visit).unwrap();
    super::super::edit(&mut shell, visit, Field::Status, "ERROR".into());
    assert!(!query.current(&shell));
    let query = Query::capture(&mut shell, visit).unwrap();
    show(&mut shell, visit, false);
    show(&mut shell, visit, true);
    assert!(!query.current(&shell));
}

#[test]
fn a_new_query_read_epoch_credentials_or_route_cannot_reuse_old_observation() {
    let (mut shell, visit) = setup();
    let first = Query::capture(&mut shell, visit).unwrap();
    let second = Query::capture(&mut shell, visit).unwrap();
    assert!(!first.current(&shell));
    assert!(second.current(&shell));
    shell.core.account_epoch += 1;
    assert!(!second.current(&shell));

    let (mut shell, visit) = setup();
    let query = Query::capture(&mut shell, visit).unwrap();
    shell
        .core
        .session
        .as_mut()
        .unwrap()
        .d_cookie
        .push_str("-rotated");
    assert!(!query.current(&shell));

    let (mut shell, visit) = setup();
    let query = Query::capture(&mut shell, visit).unwrap();
    shell.connection.routed_at = None;
    assert!(!query.current(&shell));
    assert!(Query::capture(&mut shell, visit).is_err());
}

#[test]
fn cooldown_stops_new_reads_but_preserves_sent_query_and_visible_input_custody() {
    let (mut shell, visit) = setup();
    let query = Query::capture(&mut shell, visit).unwrap();
    assert!(query.can_dispatch(&shell));
    navigation::retain_limit(&mut shell, &query.owner.ctx, Some(60));
    assert!(cooling(&shell, &query.owner.ctx));
    assert!(query.current(&shell));
    assert!(!query.can_dispatch(&shell));
    assert!(Query::capture(&mut shell, visit).is_err());
    edit(&mut shell, visit, "Edited while cooling".into());
    assert_eq!(
        document(&shell, visit).unwrap().activity.user_search,
        "Edited while cooling"
    );
    assert!(!query.current(&shell));
}

#[test]
fn typed_cached_members_and_retained_labels_cannot_replace_picker_result_authority() {
    let (mut s, visit) = setup();
    let id = s
        .app_home
        .about
        .management
        .surface
        .as_ref()
        .unwrap()
        .owner
        .ctx
        .session
        .user_id
        .clone();
    assert!(
        s.core.workspaces[&s.core.active_team.clone().unwrap()]
            .users
            .contains_key(&id)
    );
    document_mut(&mut s, visit)
        .unwrap()
        .activity
        .member_selection = Some((id.clone(), "Old selected label".into()));
    let revision = document(&s, visit).unwrap().activity.filters.revision;
    lookup::choose(&mut s, visit, 0, revision, id);
    assert!(
        document(&s, visit)
            .unwrap()
            .activity
            .filters
            .user
            .is_empty()
    );
    assert!(view(&s, visit).1.is_empty());
}

#[test]
fn late_anyone_event_cannot_clear_a_newer_filter_revision() {
    let (mut s, visit) = setup();
    let old = document(&s, visit).unwrap().activity.filters.revision;
    let id = s
        .app_home
        .about
        .management
        .surface
        .as_ref()
        .unwrap()
        .owner
        .ctx
        .session
        .user_id
        .clone();
    super::super::edit(&mut s, visit, Field::User, id.clone());
    lookup::choose(&mut s, visit, 0, old, String::new());
    assert_eq!(document(&s, visit).unwrap().activity.filters.user, id);
}

#[test]
fn composition_retains_input_but_retires_results_until_a_new_query() {
    let (mut s, visit) = setup();
    let q = Query::capture(&mut s, visit).unwrap();
    composition(&mut s, visit, true);
    edit(&mut s, visit, "Retained composition".into());
    assert!(!q.current(&s));
    assert!(Query::capture(&mut s, visit).is_err());
    assert_eq!(
        document(&s, visit).unwrap().activity.user_search,
        "Retained composition"
    );
    composition(&mut s, visit, false);
    let next = Query::capture(&mut s, visit).unwrap();
    assert!(next.current(&s));
    assert!(!q.current(&s));
}
