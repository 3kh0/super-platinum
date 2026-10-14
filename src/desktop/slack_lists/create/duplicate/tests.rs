use super::*;
use serde_json::json;

fn shell() -> ShellState {
    let mut shell =
        ShellState::fixture_variant(crate::media::MediaRegistry::default(), "slack-list");
    fixture::add_duplicate_views(&mut shell).unwrap();
    open(&mut shell);
    shell
}
fn open(shell: &mut ShellState) -> u64 {
    let view = shell.slack_lists.view.as_ref().unwrap();
    let (visit, layout) = (view.generation, view.layout.generation);
    let instance = view
        .rows
        .iter()
        .find(|row| row.id == "R_LIST_002")
        .unwrap()
        .instance_id
        .clone();
    assert!(record_detail::open_shell(shell, &instance, visit, layout));
    generation(shell)
}
fn generation(shell: &ShellState) -> u64 {
    record_detail::presentation(shell.slack_lists.view.as_ref().unwrap())
        .unwrap()
        .generation
}
fn data(shell: &ShellState) -> Loaded {
    shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .loaded
        .clone()
        .unwrap()
}
fn begin_current(shell: &mut ShellState) -> Attempt {
    let generation = generation(shell);
    begin(shell, generation).unwrap()
}

#[test]
fn duplicate_is_single_delivery_and_readback_opens_only_server_identity() {
    let mut shell = shell();
    let attempt = begin_current(&mut shell);
    assert_eq!(attempt.intent.source_record_id(), Some("R_LIST_002"));
    assert!(!attempt.intent.position().is_empty());
    assert!(begin(&mut shell, attempt.origin.as_ref().unwrap().generation).is_err());
    fixture::dispatch_creation(&mut shell, attempt.clone());
    assert_eq!(fixture::creation_attempts(&shell), 1);
    assert!(fixture::creation_prepared_valid(&shell));
    let loaded = data(&shell);
    assert!(validate(&mut shell, &attempt, &loaded).is_err());
    assert_eq!(fixture::creation_attempts(&shell), 1);
    let id = fixture::complete_creation(&mut shell).unwrap();
    assert_ne!(id, "R_LIST_002");
    fixture::refresh(&mut shell);
    assert!(!blocks_mutation(&shell));
    assert_eq!(
        record_detail::presentation(shell.slack_lists.view.as_ref().unwrap())
            .unwrap()
            .record_id,
        id
    );
    assert_eq!(
        presentation(&shell).notice.as_deref(),
        Some("Item duplicated.")
    );
}

#[test]
fn source_authority_and_fresh_preflight_reject_stale_duplicate() {
    for change in [
        "closed",
        "reopened",
        "revision",
        "selection",
        "account",
        "source",
        "position",
        "limit",
    ] {
        let mut shell = shell();
        let attempt = begin_current(&mut shell);
        let mut loaded = data(&shell);
        match change {
            "closed" | "reopened" => {
                record_detail::dismiss_shell(&mut shell);
                if change == "reopened" {
                    open(&mut shell);
                }
            }
            "revision" => shell.slack_lists.view.as_mut().unwrap().revision = next_id(),
            "selection" => {
                shell
                    .slack_lists
                    .view
                    .as_mut()
                    .unwrap()
                    .layout
                    .selection_generation = next_id()
            }
            "account" => shell.core.account_epoch += 1,
            "source" | "position" => {
                let mut raw = loaded.records.raw.clone();
                if change == "source" {
                    raw["records"][2]["fields"][0]["text"] = json!("Changed source");
                } else {
                    raw["records"][3]["position"] = json!("0000000350");
                }
                loaded.records = RecordsPage::parse(raw).unwrap();
            }
            "limit" => {
                let mut raw = loaded.document.raw.clone();
                raw["list_limits"]["over_row_maximum"] = json!(true);
                loaded.document = Document::parse(raw).unwrap();
            }
            _ => unreachable!(),
        }
        assert!(validate(&mut shell, &attempt, &loaded).is_err(), "{change}");
        assert_eq!(fixture::creation_attempts(&shell), 0);
    }
}

#[test]
fn uncertain_duplicate_keeps_add_duplicate_and_delete_locked_without_replay() {
    let mut shell = shell();
    let attempt = begin_current(&mut shell);
    fixture::set_creation_uncertain(&mut shell);
    fixture::dispatch_creation(&mut shell, attempt.clone());
    fixture::complete_creation(&mut shell).unwrap();
    fixture::refresh(&mut shell);
    assert!(blocks_mutation(&shell));
    assert!(reason(&shell).is_some());
    assert!(!delete::presentation(&shell).enabled);
    assert!(presentation(&shell).operation.unwrap().can_retire);
    let loaded = data(&shell);
    assert!(validate(&mut shell, &attempt, &loaded).is_err());
    record_detail::dismiss_shell(&mut shell);
    assert!(!presentation(&shell).enabled);
    fixture::refresh(&mut shell);
    assert!(blocks_mutation(&shell));
    assert_eq!(fixture::creation_attempts(&shell), 1);
}

#[test]
fn navigating_after_dispatch_preserves_origin_and_does_not_open_duplicate() {
    let mut shell = shell();
    let attempt = begin_current(&mut shell);
    fixture::dispatch_creation(&mut shell, attempt);
    record_detail::dismiss_shell(&mut shell);
    open(&mut shell);
    let id = fixture::complete_creation(&mut shell).unwrap();
    fixture::refresh(&mut shell);
    assert_eq!(
        record_detail::presentation(shell.slack_lists.view.as_ref().unwrap())
            .unwrap()
            .record_id,
        "R_LIST_002"
    );
    assert!(!blocks_mutation(&shell));
    record_detail::dismiss_shell(&mut shell);
    let visit = shell.slack_lists.view.as_ref().unwrap().generation;
    assert!(open_result_shell(&mut shell, visit));
    assert_eq!(
        record_detail::presentation(shell.slack_lists.view.as_ref().unwrap())
            .unwrap()
            .record_id,
        id
    );
}

#[test]
fn existing_server_identity_is_uncertain_and_never_reopens_source_as_result() {
    let mut shell = shell();
    let attempt = begin_current(&mut shell);
    let loaded = data(&shell);
    validate(&mut shell, &attempt, &loaded).unwrap();
    let source = loaded.records.record("R_LIST_002").unwrap().raw.clone();
    assert!(finish(
        &mut shell,
        &attempt,
        Ok(json!({"ok":true,"record":source}))
    ));
    fixture::refresh(&mut shell);
    assert!(blocks_mutation(&shell));
    assert!(presentation(&shell).operation.unwrap().can_retire);
    assert!(presentation(&shell).notice.is_none());
    assert!(!presentation(&shell).can_open);
}

#[test]
fn preflight_rate_limit_retains_wait_without_dispatch_or_replay() {
    let mut shell = shell();
    let attempt = begin_current(&mut shell);
    rejected(
        &mut shell,
        &attempt,
        "Wait before trying again.",
        Some(&Error::RateLimited {
            retry_after_secs: Some(60),
        }),
    );
    assert!(!blocks_mutation(&shell));
    assert!(shell.slack_lists.view.as_ref().unwrap().retry_at > now());
    assert!(reason(&shell).is_some());
    assert!(begin_current_result(&mut shell).is_err());
    assert_eq!(fixture::creation_attempts(&shell), 0);
}
fn begin_current_result(shell: &mut ShellState) -> Result<Attempt, &'static str> {
    let generation = generation(shell);
    begin(shell, generation)
}
