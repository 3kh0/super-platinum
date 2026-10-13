use super::*;
use serde_json::json;
fn shell() -> ShellState {
    let mut shell =
        ShellState::fixture_variant(crate::media::MediaRegistry::default(), "slack-list");
    super::super::super::fixture::add_channel_edits(&mut shell).unwrap();
    shell
}
fn open(shell: &mut ShellState, column: &str) -> u64 {
    let visit = shell.slack_lists.view.as_ref().unwrap().generation;
    super::super::begin_shell(shell, "R_LIST_000", column, visit).unwrap();
    shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .editor
        .as_ref()
        .unwrap()
        .generation
}
#[test]
fn reference_candidates_include_private_nonmember_and_external_but_not_archived_dm_or_unknown_type()
{
    let mut shell = shell();
    let generation = open(&mut shell, "C_EDIT_CHANNELS");
    let editor = super::super::presentation(&shell).unwrap();
    assert_eq!(editor.kind, EditorKind::MultiChannel);
    assert_eq!(
        editor
            .directory
            .candidates
            .iter()
            .map(|choice| choice.id.as_str())
            .collect::<Vec<_>>(),
        vec!["C00000001", "G00000002", "C00000003"]
    );
    assert!(
        editor
            .choices
            .iter()
            .any(|choice| choice.id == "Z99999999" && choice.label == "Z99999999")
    );
    let attempt = super::super::begin_save(
        &mut shell,
        generation,
        CellEditValue::Channels(vec!["C00000001".into()]),
    )
    .unwrap();
    assert_eq!(attempt.edit.cells()[0]["channel"], json!(["C00000001"]));
    assert!(attempt.edit.cells()[0].get("user").is_none());
}
#[test]
fn only_special_private_record_channels_require_membership() {
    let mut shell = shell();
    let team = shell.core.active_team.clone().unwrap();
    let workspace = shell.core.workspaces.get_mut(&team).unwrap();
    for (id, record, private, member) in [
        ("C10000001", "S:a:b", true, false),
        ("C10000002", "S:a:b", true, true),
        ("C10000003", "other:a:b", true, false),
        ("C10000004", "S:a:b", false, false),
    ] {
        let channel=serde_json::from_value(json!({"id":id,"name":id,"is_channel":true,"is_private":private,"is_member":member,"properties":{"record_channel":{"record_id":record}}})).unwrap();
        workspace.channels.insert(id.into(), channel);
    }
    let result = cached(&shell, Kind::Channels, "C100")
        .into_iter()
        .map(|choice| choice.id)
        .collect::<Vec<_>>();
    assert_eq!(result, vec!["C10000002", "C10000003", "C10000004"]);
}
#[test]
fn channel_remote_fuzzy_matches_are_bounded_and_wrong_domain_cannot_publish() {
    let mut shell = shell();
    let generation = open(&mut shell, "C_EDIT_CHANNEL");
    let request = begin(&mut shell, generation, "alpa".into()).unwrap();
    assert!(cached(&shell, Kind::Channels, "alpa").is_empty());
    finish(
        &mut shell,
        &request,
        Ok(Results::Users(vec![User {
            id: "U00000001".into(),
            name: Some("Alice".into()),
            ..User::default()
        }])),
    );
    assert!(
        super::super::presentation(&shell)
            .unwrap()
            .directory
            .candidates
            .is_empty()
    );
    let remote = (0..80)
        .map(|i| Channel {
            id: format!("C{i:08}"),
            name: Some(format!("Remote fuzzy {i}")),
            is_channel: true,
            ..Channel::default()
        })
        .collect();
    finish(&mut shell, &request, Ok(Results::Channels(remote)));
    assert_eq!(
        super::super::presentation(&shell)
            .unwrap()
            .directory
            .candidates
            .len(),
        LIMIT
    );
}
#[test]
fn channel_query_retains_scope_and_retry_guards_without_changing_canonical_cell() {
    for change in ["query", "cancel", "account", "read", "hidden"] {
        let mut shell = shell();
        let generation = open(&mut shell, "C_EDIT_CHANNEL");
        let before = super::super::super::fixture::channel_edit_value(&shell, "edit_channel");
        let request = begin(&mut shell, generation, "alpha".into()).unwrap();
        match change {
            "query" => {
                begin(&mut shell, generation, "beta".into()).unwrap();
            }
            "cancel" => {
                super::super::cancel_shell(&mut shell, generation);
            }
            "account" => shell.core.account_epoch += 1,
            "read" => shell.slack_lists.view.as_mut().unwrap().revision = next_id(),
            "hidden" => shell.slack_lists.view.as_mut().unwrap().rows = Arc::new(Vec::new()),
            _ => unreachable!(),
        }
        assert!(!request.current(&shell));
        finish(&mut shell, &request, Ok(Results::Channels(vec![])));
        assert_eq!(
            super::super::super::fixture::channel_edit_value(&shell, "edit_channel"),
            before
        );
    }
    let mut shell = shell();
    let generation = open(&mut shell, "C_EDIT_CHANNEL");
    let request = begin(&mut shell, generation, "alpha".into()).unwrap();
    finish(
        &mut shell,
        &request,
        Err(Error::RateLimited {
            retry_after_secs: Some(60),
        }),
    );
    assert!(begin(&mut shell, generation, "beta".into()).is_none());
    let editor = super::super::presentation(&shell).unwrap();
    assert!(!editor.directory.loading);
    assert_eq!(editor.directory.candidates[0].id, "G00000002");
    assert!(editor.directory.error.unwrap().contains("wait"));
}
#[test]
fn single_channel_clear_and_multi_validation_reuse_cell_guard() {
    let mut shell = shell();
    let generation = open(&mut shell, "C_EDIT_CHANNEL");
    assert!(
        super::super::begin_save(
            &mut shell,
            generation,
            CellEditValue::Channels(vec!["C00000001".into(), "G00000002".into()])
        )
        .is_err()
    );
    let attempt =
        super::super::begin_save(&mut shell, generation, CellEditValue::Channels(vec![])).unwrap();
    assert_eq!(attempt.edit.cells()[0]["channel"], json!([]));
}

#[test]
fn switching_between_people_and_channels_retires_the_previous_directory_request() {
    let mut shell = shell();
    super::super::super::fixture::add_user_edits(&mut shell).unwrap();
    let generation = open(&mut shell, "C_EDIT_USER");
    let old = begin(&mut shell, generation, "Alice".into()).unwrap();
    assert!(super::super::cancel_shell(&mut shell, generation));
    let channel = open(&mut shell, "C_EDIT_CHANNEL");
    let current = begin(&mut shell, channel, "beta".into()).unwrap();
    assert!(!old.current(&shell));
    assert!(current.current(&shell));
    finish(&mut shell, &old, Ok(Results::Users(vec![])));
    let editor = super::super::presentation(&shell).unwrap();
    assert_eq!(editor.directory.query, "beta");
    assert_eq!(editor.directory.candidates[0].id, "G00000002");
}
