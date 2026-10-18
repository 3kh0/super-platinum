//! Narrow disconnected ledger for the personal calendar-date workflow.
use super::*;
use super_platinum_core::slack::client::PreparedRequest;
#[derive(Clone, Default)]
pub(crate) struct State {
    armed: bool,
    forms: Vec<Value>,
    outcomes: Vec<Value>,
}
pub(crate) fn install(shell: &mut ShellState) {
    if !super::super::check::task_date::enabled() {
        return;
    }
    assert!(enabled(shell));
    let generation = shell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .editor
        .as_ref()
        .unwrap()
        .generation;
    assert!(editor::cancel_shell(shell, generation));
    let fixture = shell.slack_lists.fixture.as_mut().unwrap();
    fixture.task_date.armed = true;
    fixture.document["list_metadata"]["views"] = json!([
        {"id":"V_TASK_DATE","name":"Calendar tasks","type":"table","position":"1000000000","columns":[{"id":"C_NAME","key":"name","visible":true},{"id":"C_DATE","key":"due","visible":true}],"filters":[],"sorts":[]},
        {"id":"V_TASK_DATE_OTHER","name":"Other tasks","type":"table","position":"2000000000","columns":[{"id":"C_NAME","key":"name","visible":true}],"filters":[],"sorts":[]}
    ]);
    refresh(shell);
}
pub(super) fn request(shell: &mut ShellState, attempt: &EditAttempt, request: &PreparedRequest) {
    if !shell
        .slack_lists
        .fixture
        .as_ref()
        .is_some_and(|f| f.task_date.armed)
    {
        return;
    }
    let fields = match &request.body {
        RequestBody::Form(fields) => json!(fields),
        _ => Value::Null,
    };
    shell.slack_lists.fixture.as_mut().unwrap().task_date.forms.push(json!({
        "method":request.method,"url":request.url,"fields":fields,"retrySafe":request.retry_safe(),
        "serial":attempt.serial,"cells":attempt.edit.cells()
    }));
}
pub(crate) fn complete(shell: &mut ShellState, lost: bool) -> Result<(), String> {
    let fixture = shell.slack_lists.fixture.as_mut().ok_or("No fixture")?;
    if !fixture.task_date.armed || fixture.pending.is_none() {
        return Err("No owned date write".into());
    }
    let serial = fixture.pending.as_ref().unwrap().serial;
    fixture.uncertain = lost;
    complete_pending(shell);
    shell
        .slack_lists
        .fixture
        .as_mut()
        .unwrap()
        .task_date
        .outcomes
        .push(json!({"serial":serial,"lost":lost}));
    Ok(())
}
pub(crate) fn policy_change(shell: &mut ShellState) {
    assert!(enabled(shell) && shell.slack_lists.fixture.as_ref().unwrap().task_date.armed);
    // Change only the independent backend policy, then use the normal read path.
    shell
        .slack_lists
        .fixture
        .as_mut()
        .unwrap()
        .experiments
        .todos = true;
    refresh(shell);
}
pub(crate) fn snapshot(shell: &ShellState) -> Result<Value, String> {
    if !enabled(shell) || shell.core.session.is_some() || shell.core.transport.is_some() {
        return Err("Disconnected date fixture required".into());
    }
    let fixture = shell.slack_lists.fixture.as_ref().ok_or("No fixture")?;
    if !fixture.task_date.armed {
        return Err("Date ledger not armed".into());
    }
    let view = shell.slack_lists.view.as_ref().ok_or("No date view")?;
    let loaded = view.loaded.as_ref().ok_or("No date records")?;
    let mutation = shell.slack_lists.operations.get(&view.id).map(|m| json!({
        "serial":m.attempt.serial,"visit":m.attempt.generation,"selection":m.attempt.selection_generation,
        "phase":match m.phase {Phase::Preparing=>"Preparing",Phase::Saving=>"Saving",Phase::Uncertain=>"Uncertain"},
        "cells":m.attempt.edit.cells()
    }));
    let editor = editor::presentation(shell).map(|e| json!({"generation":e.generation,"row":e.row_id,"column":e.column_id,"disabled":e.disabled,"saving":e.saving,"stale":e.stale,"error":e.error}));
    Ok(
        json!({"document":loaded.document.raw,"population":loaded.records.raw,"backend":fixture.records,
        "visit":view.generation,"revision":view.revision,"selection":view.layout.selection_generation,"selected":view.layout.selected,
        "find":{"input":view.layout.find.input,"applied":view.layout.find.applied,"revision":view.layout.find.revision},
        "forms":fixture.task_date.forms,"outcomes":fixture.task_date.outcomes,"pending":fixture.pending.is_some(),"mutation":mutation,"editor":editor,
        "todos":loaded.list_experiments.as_ref().map(|e|e.todos),"attempts":fixture.attempts}),
    )
}

pub(crate) fn request_url(value: &str) -> bool {
    url(value, "lists.cells.update")
}

// XParams::rest_pairs emits three fresh UUIDs; retain them, validate their
// exact lexical shape and every other ordered pair, never regenerate IDs.
pub(super) fn url(value: &str, method: &str) -> bool {
    let Some((base, query)) = value.split_once('?') else {
        return false;
    };
    if base != format!("https://example.test/api/{method}") || query.contains('#') {
        return false;
    }
    let pairs = query
        .split('&')
        .map(|part| part.split_once('='))
        .collect::<Option<Vec<_>>>();
    let Some(pairs) = pairs else {
        return false;
    };
    if pairs.len() != 10 {
        return false;
    }
    let expected = [
        ("_x_id", None),
        ("_x_csid", None),
        ("_x_desktop_ia", Some("true")),
        ("_x_foreground", Some("true")),
        ("_x_frontend_build_type", Some("current")),
        ("_x_gantry", Some("true")),
        ("_x_num_retries", Some("0")),
        ("_x_version_ts", Some("capture-required")),
        ("fp", None),
        ("slack_route", Some("default")),
    ];
    pairs
        .iter()
        .zip(expected)
        .all(|((key, value), (name, literal))| {
            *key == name
                && match literal {
                    Some(v) => *value == v,
                    None => {
                        value.len() == 32
                            && value
                                .bytes()
                                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
                            && value.as_bytes()[12] == b'4'
                            && matches!(value.as_bytes()[16], b'8' | b'9' | b'a' | b'b')
                    }
                }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn shell(armed: bool) -> ShellState {
        let mut shell =
            ShellState::fixture_variant(crate::media::MediaRegistry::default(), "slack-list");
        editor::date::fixture::seed(&mut shell, "todo_due_date", true);
        shell.slack_lists.fixture.as_mut().unwrap().task_date.armed = armed;
        shell
    }
    fn prepare(shell: &mut ShellState, value: CellEditValue) -> (u64, EditAttempt) {
        let generation = shell
            .slack_lists
            .view
            .as_ref()
            .unwrap()
            .editor
            .as_ref()
            .unwrap()
            .generation;
        let attempt = editor::begin_save(shell, generation, value).unwrap();
        (generation, attempt)
    }
    #[test]
    fn actual_dispatch_observer_is_opt_in_and_records_prepared_form() {
        for armed in [false, true] {
            let mut shell = shell(armed);
            let (_, attempt) = prepare(&mut shell, CellEditValue::Date(None));
            let serial = attempt.serial;
            super::super::dispatch(&mut shell, attempt);
            let fixture = shell.slack_lists.fixture.as_ref().unwrap();
            assert_eq!(fixture.attempts, 1);
            assert_eq!(fixture.task_date.forms.len(), usize::from(armed));
            if armed {
                let form = &fixture.task_date.forms[0];
                assert_eq!(form["serial"], serial);
                assert_eq!(form["method"], "POST");
                let url = form["url"].as_str().unwrap();
                assert!(request_url(url));
                for wrong in [
                    url.split('?').next().unwrap().to_owned(),
                    format!("{url}&extra=1"),
                    format!("{url}#fragment"),
                    url.replace("example.test", "foreign.test"),
                    url.replace("_x_num_retries=0", "_x_num_retries=1"),
                    url.replacen("_x_id=", "_x_csid=", 1),
                    url.replace("capture-required", "capture%2Drequired"),
                ] {
                    assert!(!request_url(&wrong));
                }
                assert_eq!(form["retrySafe"], false);
                let pairs = form["fields"].as_array().unwrap();
                assert_eq!(pairs.len(), 4);
                assert_eq!(pairs[2][0], "cells");
                assert_eq!(
                    serde_json::from_str::<Value>(pairs[2][1].as_str().unwrap()).unwrap(),
                    form["cells"]
                );
                assert_eq!(
                    form["cells"],
                    json!([{"row_id":"R_DATE","column_id":"C_DATE","date":[]}])
                );
            }
        }
    }
    #[test]
    fn lost_write_after_cancel_preserves_receipt_until_actual_refresh_without_replay() {
        let mut shell = shell(true);
        let original = snapshot(&shell).unwrap();
        let (generation, attempt) =
            prepare(&mut shell, CellEditValue::Date(Some("2026-10-18".into())));
        super::super::dispatch(&mut shell, attempt);
        assert!(editor::cancel_shell(&mut shell, generation));
        complete(&mut shell, true).unwrap();
        let held = snapshot(&shell).unwrap();
        assert_eq!(held["forms"].as_array().unwrap().len(), 1);
        assert_eq!(held["mutation"]["phase"], "Uncertain");
        assert_eq!(held["population"], original["population"]);
        assert_eq!(held["editor"], Value::Null);
        assert!(complete(&mut shell, true).is_err());
        refresh(&mut shell);
        let observed = snapshot(&shell).unwrap();
        assert_eq!(observed["forms"], held["forms"]);
        assert_eq!(observed["outcomes"], held["outcomes"]);
        assert_eq!(observed["mutation"], Value::Null);
        assert_eq!(observed["document"], original["document"]);
        assert_eq!(observed["population"], held["backend"]);
        assert_eq!(
            observed["population"]["response_metadata"],
            original["population"]["response_metadata"]
        );
        assert_eq!(
            observed["population"]["records"][0]["fields"][1]["timestamp"],
            json!([-1])
        );
    }
}
