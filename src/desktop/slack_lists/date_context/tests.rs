use super::*;
use serde_json::json;

fn shell() -> ShellState {
    let mut shell =
        ShellState::fixture_variant(crate::media::MediaRegistry::default(), "slack-list");
    fixture::add_saved_views(&mut shell);
    let view = shell.slack_lists.view.as_ref().unwrap();
    let (visit, generation) = (view.generation, view.layout.generation);
    assert!(layout::select_shell(
        &mut shell,
        Some("V_LIST_ALL".into()),
        visit,
        generation
    ));
    shell
}
fn context(date: &str) -> DateContext {
    DateContext::from_local(date, None).unwrap()
}
#[test]
fn calendar_views_reproject_once_per_minute_even_with_identical_current_date_and_offset() {
    let mut shell = shell();
    let view = shell.slack_lists.view.as_mut().unwrap();
    // Use a real date-dependent saved view, so rebuilding retains its sampling
    // dependency. Historical timestamp conversions may change after an OS zone
    // switch even when the sampled current date and offset remain identical.
    let loaded = view.loaded.as_mut().unwrap();
    let mut raw = loaded.document.raw.clone();
    raw["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id":"C_CREATED_TEST","key":"created_time","name":"Created","type":"created_time"
        }));
    let selected = raw["list_metadata"]["views"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|v| v["id"] == "V_LIST_ALL")
        .unwrap();
    selected["filters"] =
        json!([{"key":"created_time","operator":"greater_than","values":["1970-01-01"]}]);
    loaded.document = Document::parse(raw).unwrap();
    let deadline = Instant::now();
    view.layout.dates.dependencies = DateDependencies {
        calendar: true,
        ..Default::default()
    };
    let sample = context("2026-10-13T12:00:00");
    view.layout.dates.context = Some(sample.clone());
    view.layout.dates.next_sample = Some(deadline);
    let rows = view.rows.clone();
    assert!(!poll_with(
        &mut shell,
        deadline - Duration::from_secs(1),
        |_| panic!("sampled before deadline")
    ));
    assert!(std::sync::Arc::ptr_eq(
        &rows,
        &shell.slack_lists.view.as_ref().unwrap().rows
    ));
    assert!(poll_with(&mut shell, deadline, |_| sample.clone()));
    let view = shell.slack_lists.view.as_ref().unwrap();
    assert!(!std::sync::Arc::ptr_eq(&rows, &view.rows));
    assert_eq!(
        view.layout.dates.context.as_ref().unwrap().calendar_key(),
        sample.calendar_key()
    );
    let rows = view.rows.clone();
    let next = view
        .layout
        .dates
        .next_sample
        .expect("date rule keeps bounded sampling");
    assert!(!poll_with(
        &mut shell,
        next - Duration::from_secs(1),
        |_| panic!("sampled before next minute")
    ));
    assert!(std::sync::Arc::ptr_eq(
        &rows,
        &shell.slack_lists.view.as_ref().unwrap().rows
    ));
    assert!(poll_with(&mut shell, next, |_| sample));
    assert!(!std::sync::Arc::ptr_eq(
        &rows,
        &shell.slack_lists.view.as_ref().unwrap().rows
    ));
}
#[test]
fn clock_dependence_and_timezone_changes_refresh_only_at_due_sample() {
    for clock in [false, true] {
        let mut shell = shell();
        let deadline = Instant::now();
        let view = shell.slack_lists.view.as_mut().unwrap();
        view.layout.dates.dependencies = DateDependencies {
            calendar: true,
            clock,
            ..Default::default()
        };
        view.layout.dates.context = Some(context("2026-10-13T12:00:00"));
        view.layout.dates.next_sample = Some(deadline);
        let updated = if clock {
            context("2026-10-13T12:01:00")
        } else {
            DateContext::from_local_with_offset("2026-10-13T12:00:00", None, 3600).unwrap()
        };
        assert!(poll_with(&mut shell, deadline, |_| updated));
    }
}
#[test]
fn fixed_fixtures_hidden_dialogs_and_old_accounts_never_sample() {
    let mut shell = shell();
    assert!(set_override(&mut shell, context("2026-10-13T12:00:00")));
    assert!(!poll_with(
        &mut shell,
        Instant::now() + SAMPLE_INTERVAL,
        |_| panic!("fixed clock sampled")
    ));
    shell
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .layout
        .dates
        .next_sample = Some(Instant::now());
    shell.overlay = None;
    assert!(!poll_with(
        &mut shell,
        Instant::now() + SAMPLE_INTERVAL,
        |_| panic!("hidden view sampled")
    ));
    shell.overlay = Some(Overlay::SlackList);
    shell.core.account_epoch += 1;
    assert!(!poll_with(
        &mut shell,
        Instant::now() + SAMPLE_INTERVAL,
        |_| panic!("old account sampled")
    ));
}
#[test]
fn timestamp_preference_never_substitutes_grid_experiment_for_todos() {
    let shell = shell();
    let mut loaded = shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .clone()
        .unwrap();
    loaded.personal_task_lists = PersonalTaskLists::from_response(
        &json!({"ok":true,"prefs":{"task_lists":"[\"F_LIST_FIXTURE\"]","assigned_to_list":""}}),
    )
    .ok();
    loaded.list_experiments = None;
    assert_eq!(uses_timestamps(&loaded), None);
    loaded.list_experiments = Some(super_platinum_core::slack::api::ListExperiments {
        grid_todos: true,
        todos: false,
        ..Default::default()
    });
    assert_eq!(uses_timestamps(&loaded), Some(false));
    let flags = loaded.list_experiments.as_mut().unwrap();
    flags.todos = true;
    flags.grid_todos = false;
    assert_eq!(uses_timestamps(&loaded), Some(true));
    loaded.list_experiments.as_mut().unwrap().todos_rollback = true;
    assert_eq!(uses_timestamps(&loaded), Some(false));
}

fn timestamp_shell() -> ShellState {
    let mut shell = shell();
    fixture::add_date_views(&mut shell);
    let view = shell.slack_lists.view.as_ref().unwrap();
    let (visit, generation) = (view.generation, view.layout.generation);
    assert!(layout::select_shell(&mut shell, None, visit, generation));
    fixture::set_date_clock(&mut shell, "2026-10-12T12:02:00", "2026-10-12T12:00:00");
    shell
}
fn stamp(shell: &ShellState) -> CellView {
    shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .all_rows
        .iter()
        .find(|row| row.id == "R_LIST_000")
        .unwrap()
        .cells
        .iter()
        .find(|cell| cell.column_id == "C_STAMP")
        .unwrap()
        .clone()
}
#[test]
fn ordinary_timestamp_cells_refresh_at_due_minute_without_canonical_changes_or_unrelated_row_clones()
 {
    let mut shell = timestamp_shell();
    assert_eq!(stamp(&shell).text, "2 minutes ago");
    assert_eq!(
        stamp(&shell).content,
        CellContent::Timestamp {
            tooltip: "Today at 12:00:00 PM".into()
        }
    );
    assert!(!stamp(&shell).can_edit && !stamp(&shell).editable);
    let view = shell.slack_lists.view.as_ref().unwrap();
    let raw = view.loaded.as_ref().unwrap().records.raw.clone();
    let (visit, revision, layout) = (view.generation, view.revision, view.layout.generation);
    let all_rows = view.all_rows.clone();
    let unrelated = view
        .all_rows
        .iter()
        .find(|row| row.id == "R_LIST_003")
        .unwrap()
        .clone();
    let deadline = Instant::now();
    let dates = &mut shell.slack_lists.view.as_mut().unwrap().layout.dates;
    dates.fixed = None;
    dates.next_sample = Some(deadline);
    assert!(!poll_with(
        &mut shell,
        deadline - Duration::from_secs(1),
        |_| panic!("early clock sample")
    ));
    assert!(std::sync::Arc::ptr_eq(
        &all_rows,
        &shell.slack_lists.view.as_ref().unwrap().all_rows
    ));
    shell.overlay = None;
    assert!(!poll_with(&mut shell, deadline, |_| panic!(
        "hidden timestamp view sampled"
    )));
    assert!(std::sync::Arc::ptr_eq(
        &all_rows,
        &shell.slack_lists.view.as_ref().unwrap().all_rows
    ));
    shell.overlay = Some(Overlay::SlackList);
    assert!(poll_with(&mut shell, deadline, |_| {
        DateContext::from_local_with_offset(
            "2026-10-12T12:03:00",
            Some("2026-10-12T12:00:00"),
            -14400,
        )
        .unwrap()
    }));
    assert_eq!(stamp(&shell).text, "3 minutes ago");
    let view = shell.slack_lists.view.as_ref().unwrap();
    assert!(
        view.layout.dates.next_sample.is_some(),
        "All loaded items retains minute dependency"
    );
    assert_eq!(
        (view.generation, view.revision, view.layout.generation),
        (visit, revision, layout)
    );
    assert_eq!(view.loaded.as_ref().unwrap().records.raw, raw);
    assert!(std::sync::Arc::ptr_eq(
        &unrelated,
        view.all_rows
            .iter()
            .find(|row| row.id == "R_LIST_003")
            .unwrap()
    ));
    assert!(editor::begin_shell(&mut shell, "R_LIST_000", "C_STAMP", visit).is_err());
    assert_eq!(fixture::attempt_count(&shell), 0);
}

#[test]
fn timestamp_preferences_and_clock_refresh_detail_without_retiring_open_editor() {
    let mut shell = timestamp_shell();
    let view = shell.slack_lists.view.as_ref().unwrap();
    let (visit, generation) = (view.generation, view.layout.generation);
    assert!(record_detail::open_shell(
        &mut shell,
        "R_LIST_000",
        visit,
        generation
    ));
    let detail = presentation(&shell).unwrap().detail.unwrap();
    editor::begin_with_authority(
        &mut shell,
        "R_LIST_000",
        "C_NUMBER",
        visit,
        EditAuthority::RecordDetail(detail.generation),
    )
    .unwrap();
    let draft = editor::presentation(&shell).unwrap();
    let raw = shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .as_ref()
        .unwrap()
        .records
        .raw
        .clone();
    let team = shell.slack_lists.scope.as_ref().unwrap().team.clone();
    shell
        .core
        .workspaces
        .get_mut(&team)
        .unwrap()
        .notifications
        .user
        .insert("time24".into(), json!(true));
    locale_changed(&mut shell, "OTHER_TEAM");
    assert_eq!(
        stamp(&shell).content,
        CellContent::Timestamp {
            tooltip: "Today at 12:00:00 PM".into()
        }
    );
    locale_changed(&mut shell, &team);
    assert_eq!(
        stamp(&shell).content,
        CellContent::Timestamp {
            tooltip: "Today at 12:00:00".into()
        }
    );
    shell.core.workspaces.get_mut(&team).unwrap().locale = Some("de-DE".into());
    locale_changed(&mut shell, &team);
    assert_eq!(stamp(&shell).text, "vor 2 Minuten");
    assert_eq!(
        stamp(&shell).content,
        CellContent::Timestamp {
            tooltip: "Heute at 12:00:00 Uhr".into()
        }
    );
    assert!(fixture::tick_date_clock(
        &mut shell,
        "2026-10-12T12:03:00",
        "2026-10-12T12:00:00"
    ));
    let updated = presentation(&shell).unwrap().detail.unwrap();
    assert_eq!(updated.generation, detail.generation);
    assert_eq!(updated.open_generation, detail.open_generation);
    assert_eq!(
        updated
            .row
            .cells
            .iter()
            .find(|cell| cell.column_id == "C_STAMP")
            .unwrap()
            .text,
        "vor 3 Minuten"
    );
    assert!(editor::presentation(&shell).unwrap() == draft);
    assert_eq!(
        shell
            .slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap()
            .records
            .raw,
        raw
    );
    assert_eq!(fixture::attempt_count(&shell), 0);
}
