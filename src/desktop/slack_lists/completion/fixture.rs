//! Named, disconnected personal/reference pair. Completion dispatch and canonical
//! observation are separately driven; an ACK never mutates displayed rows.
use super::*;
use serde_json::json;
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{api, client::RequestBody},
};
#[derive(Clone, Default)]
pub(super) struct State {
    pub(super) reference: Option<Loaded>,
    pub(super) pending: Option<(Task, bool, TaskCompletionCell)>,
    pub(super) forms: Vec<Value>,
    pub(super) reads: usize,
    completed: Vec<Value>,
}
pub(crate) fn install(shell: &mut ShellState, name: &str) {
    assert!(matches!(
        name,
        "slack-list-todo-completion" | "slack-list-todo-completion-narrow"
    ));
    super::super::fixture::install(shell, "slack-list-incomplete");
    super::super::fixture::completion_setup(shell);
    let primary = super::super::fixture::deletion_loaded(shell).unwrap();
    let mut reference = primary.clone();
    let mut doc = primary.document.raw.clone();
    doc["id"] = json!("F_TODO_SOURCE");
    doc["title"] = json!("Referenced project");
    reference.document = Document::parse(doc).unwrap();
    let mut record = primary.records.records[0].raw.clone();
    record["id"] = json!("R_SOURCE");
    record["list_id"] = json!("F_TODO_SOURCE");
    record["fields"]
        .as_array_mut()
        .unwrap()
        .retain(|field| field["key"] != "todo_ref");
    reference.records=RecordsPage::parse(json!({"ok":true,"records":[record],"response_metadata":{"next_cursor":"source-cursor-retained"}})).unwrap();
    shell.slack_lists.completion.fixture = State {
        reference: Some(reference),
        ..Default::default()
    };
    let view = shell.slack_lists.view.as_ref().unwrap();
    let (visit, layout, row) = (
        view.generation,
        view.layout.generation,
        view.rows[0].instance_id.clone(),
    );
    assert!(record_detail::open_shell(shell, &row, visit, layout));
}
fn primary(shell: &ShellState) -> Loaded {
    super::super::fixture::deletion_loaded(shell).unwrap()
}
fn queue(shell: &mut ShellState, task: Task, reference: bool, cell: TaskCompletionCell) {
    let session = WorkspaceSession {
        team_id: "T_LIST_FIXTURE".into(),
        enterprise_id: None,
        user_id: "U_LIST_FIXTURE".into(),
        name: "Fixture".into(),
        url: "https://fixture.slack.com".into(),
        token: "fixture-not-a-token".into(),
    };
    let request = api::list_cell_update(&shell.core.client, &session, cell.edit());
    let RequestBody::Form(form) = request.body else {
        panic!("cell form")
    };
    assert_eq!(form[0], ("token".into(), "fixture-not-a-token".into()));
    assert_eq!(form[1], ("list_id".into(), cell.edit().list_id().into()));
    assert_eq!(form[2], ("cells".into(), cell.edit().cells().to_string()));
    assert_eq!(form[3], ("_x_reason".into(), "bulk-update-cells".into()));
    assert_eq!(form.len(), 4);
    shell
        .slack_lists
        .completion
        .fixture
        .forms
        .push(json!({"method":"lists.cells.update","fields":&form[1..]}));
    shell.slack_lists.completion.fixture.pending = Some((task, reference, cell));
}
pub(super) fn start(shell: &mut ShellState, task: Task) {
    if !super::super::fixture::enabled(shell) {
        return;
    }
    if !task.current(shell) {
        failed(
            shell,
            &task,
            "This To-do visit changed; no completion was sent.",
            None,
        );
        return;
    }
    let primary = primary(shell);
    let reference = shell.slack_lists.completion.fixture.reference.clone();
    shell.slack_lists.completion.fixture.reads += 1 + usize::from(reference.is_some());
    if let Err(message) = prepare_primary(shell, &task, &primary, reference.as_ref()) {
        failed(shell, &task, message, None);
        return;
    }
    let cell = shell.slack_lists.completion.entries[&task.serial]
        .intent
        .primary()
        .clone();
    queue(shell, task, false, cell);
}
pub(super) fn reference(shell: &mut ShellState, task: Task) {
    if !super::super::fixture::enabled(shell) {
        return;
    }
    if !task.current(shell) {
        failed(
            shell,
            &task,
            "This To-do visit changed; no referenced completion was sent.",
            None,
        );
        return;
    }
    let primary = primary(shell);
    let reference = shell
        .slack_lists
        .completion
        .fixture
        .reference
        .clone()
        .unwrap();
    shell.slack_lists.completion.fixture.reads += 2;
    match prepare_reference(shell, &task, &primary, &reference) {
        Ok(cell) => queue(shell, task, true, cell),
        Err(message) => failed(shell, &task, message, None),
    }
}
pub(crate) fn complete(shell: &mut ShellState, lost_reply: bool, apply: bool) -> bool {
    if !super::super::fixture::enabled(shell) {
        return false;
    }
    let Some((task, reference, cell)) = shell.slack_lists.completion.fixture.pending.take() else {
        return false;
    };
    let super_platinum_core::slack_lists::CellEditValue::Boolean(checked) = cell.edit().value()
    else {
        unreachable!()
    };
    if apply {
        if reference {
            let target = shell
                .slack_lists
                .completion
                .fixture
                .reference
                .as_mut()
                .unwrap();
            let mut raw = target.records.raw.clone();
            let field = raw["records"][0]["fields"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|f| f["key"] == "todo_completed")
                .unwrap();
            field["checkbox"] = json!(checked);
            target.records = RecordsPage::parse(raw).unwrap();
        } else {
            super::super::fixture::completion_observe(shell, cell.edit().record_id(), checked);
        }
    }
    let result = if lost_reply {
        Err(Error::Transport("fixture lost reply".into()))
    } else {
        Ok(json!({"ok":true,"cells":cell.edit().cells()}))
    };
    shell.slack_lists.completion.fixture.completed.push(json!({"reference":reference,"cells":cell.edit().cells(),"lostReply":lost_reply,"applied":apply}));
    settled(shell, &task, reference, result)
}
pub(super) fn check(shell: &mut ShellState, task: Task, continue_reference: bool) {
    if !super::super::fixture::enabled(shell) {
        return;
    }
    if !task.current(shell) {
        failed(shell, &task, "This check belongs to an older visit.", None);
        return;
    }
    let primary = primary(shell);
    let reference = shell.slack_lists.completion.fixture.reference.clone();
    shell.slack_lists.completion.fixture.reads += 1 + usize::from(reference.is_some());
    match observed(shell, &task, &primary, reference.as_ref()) {
        Observed::Complete => super::super::fixture::refresh(shell),
        Observed::ReferenceReady if continue_reference => {
            if let Some(next) = continuation_task(shell, &task) {
                self::reference(shell, next);
            }
        }
        _ => {}
    }
}

/// No synthetic form/result: these are the queued builder outputs and reducer entries.
pub(crate) fn diagnostic(shell: &ShellState) -> Result<Value, String> {
    if !crate::slack_lists::check::todo::enabled()
        || !super::super::fixture::enabled(shell)
        || shell.core.transport.is_some()
        || shell.core.session.is_some()
    {
        return Err("Disconnected To-do diagnostic required".into());
    }
    let f = &shell.slack_lists.completion.fixture;
    let entries=shell.slack_lists.completion.entries.values().map(|e|json!({"serial":e.serial,
        "list":e.intent.primary().edit().list_id(),"row":e.intent.primary().edit().record_id(),
        "phase":match e.phase {Phase::Preparing=>"Preparing",Phase::PrimarySending=>"PrimarySending",Phase::PrimaryUnknown=>"PrimaryUnknown",Phase::ReferenceReady=>"ReferenceReady",Phase::ReferenceSending=>"ReferenceSending",Phase::ReferenceUnknown=>"ReferenceUnknown",Phase::Stopped=>"Stopped"},
        "running":e.running,"primaryObserved":e.primary_observed,"cells":e.intent.primary().edit().cells(),
        "referenceCells":e.reference.as_ref().map(|c|c.edit().cells())})).collect::<Vec<_>>();
    Ok(
        json!({"forms":f.forms,"completions":f.completed,"reads":f.reads,"pending":f.pending.is_some(),"entries":entries,
        "backend":primary(shell).records.raw,"reference":f.reference.as_ref().map(|l|json!({"document":l.document.raw,"records":l.records.raw})),
        "presentation":super::presentation(shell).map(|p|json!({"serial":p.serial,"pending":p.pending,"canCheck":p.can_check,"canContinue":p.can_continue,"canDismiss":p.can_dismiss,"message":p.message}))}),
    )
}
