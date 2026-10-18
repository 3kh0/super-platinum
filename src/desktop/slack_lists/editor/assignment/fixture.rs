//! Static disconnected review seed. The dynamic transport script is a separate
//! acceptance step; this seed never constructs a transport or grants access.
use super::*;
use serde_json::json;
pub(crate) const NAMES: [&str; 4] = [
    "slack-list-task-assignment",
    "slack-list-task-assignment-narrow",
    "slack-list-task-assignment-recovery",
    "slack-list-task-assignment-recovery-narrow",
];
pub(crate) fn install(shell: &mut ShellState, name: &str) {
    assert!(NAMES.contains(&name));
    crate::slack_lists::fixture::install(shell, name);
    shell.core.active_account = Some(format!("assignment-fixture-{}", next_id()));
    shell.slack_lists.scope = Scope::capture(shell);
    assert!(seed(shell));
    if name.contains("-recovery") {
        retain_detached(shell);
    }
}
pub(crate) fn seed(shell: &mut ShellState) -> bool {
    let Some(view) = shell.slack_lists.view.as_mut() else {
        return false;
    };
    let Some(loaded) = view.loaded.as_mut() else {
        return false;
    };
    let id = loaded.document.id.clone();
    loaded.document=Document::parse(json!({"id":id,"mode":"list","title":"Assignment review","access":"write","is_locked":false,"created":1,"user":"U1","list_metadata":{"schema":[
        {"id":"C_NAME","key":"name","name":"Item","type":"rich_text","is_primary_column":true},
        {"id":"C_TASK","key":"assignee","name":"Assignee","type":"todo_assignee","options":{"format":"multi_entity"}}
    ],"views":[],"opaque":{"retain":true}}})).unwrap();
    loaded.records=RecordsPage::parse(json!({"ok":true,"records":[{"id":"R_TASK","list_id":id,"date_created":1,"created_by":"U1","fields":[
        {"key":"name","rich_text":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Review the launch plan"}]}]}]},
        {"key":"assignee","user":["U00000001"],"opaque":{"retain":true}}
    ]}],"response_metadata":{"next_cursor":"retained-page-cursor"},"opaque":17})).unwrap();
    loaded.personal_task_lists = Some(
        PersonalTaskLists::from_response(
            &json!({"ok":true,"prefs":{"task_lists":"[]","assigned_to_list":""}}),
        )
        .unwrap(),
    );
    loaded.allow_lists = Some("full".into());
    view.layout = Default::default();
    view.editor = None;
    view.stale = false;
    view.loading = false;
    view.unavailable = false;
    let document = loaded.document.raw.clone();
    let records = loaded.records.raw.clone();
    crate::slack_lists::fixture::replace_assignment_seed(shell, document, records);
    rebuild_rows(shell);
    let visit = shell.slack_lists.view.as_ref().unwrap().generation;
    if super::super::begin_shell(shell, "R_TASK", "C_TASK", visit).is_err() {
        return false;
    }
    let generation = shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .editor
        .as_ref()
        .unwrap()
        .generation;
    let Ok(guard) = begin(
        shell,
        generation,
        CellEditValue::Users(vec![
            "U00000001".into(),
            "U00000002".into(),
            "W00000003".into(),
        ]),
    ) else {
        return false;
    };
    guard.update(|e|{
        for (id,label) in [("U00000002","Bob Guest"),("W00000003","Whitney External")] {
            e.intent.record_access(id,&json!({"ok":true,"can_see":false})).unwrap();
            e.recipients.push(Recipient{id:id.into(),label:label.into(),grant:AssigneeGrant::Write,result:ShareResult::NotAttempted,dm_attempted:false,channel:None});
        }
        e.stage=Stage::Review;
        e.message="These people do not have access to this List. Share with eligible people, or choose Not Now to assign without sharing.".into();
    });
    true
}

/// Disconnected recovery seed: a sent grant remains unconfirmed after file
/// metadata changed and the originating editor was closed. No transport exists.
pub(crate) fn retain_detached(shell: &mut ShellState) {
    let e = entry(shell).unwrap();
    let guard = Guard::capture(shell, &e).unwrap();
    guard.update(|e| {
        e.recipients[0].result = ShareResult::Uncertain;
        e.recipients[0].dm_attempted = true;
        e.stage = Stage::Uncertain;
        e.message = "The List changed. The earlier sharing result is retained.".into();
    });
    let loaded = shell
        .slack_lists
        .view
        .as_mut()
        .unwrap()
        .loaded
        .as_mut()
        .unwrap();
    loaded.document.raw["shares"] = json!({"private":{"D00000002":[]}});
    let document = loaded.document.raw.clone();
    let records = loaded.records.raw.clone();
    crate::slack_lists::fixture::replace_assignment_seed(shell, document, records);
    assert!(super::super::cancel_shell(shell, guard.draft));
}

/// Closed dynamic diagnostic I/O, separate from static review seeds.
pub(crate) mod dynamic;
