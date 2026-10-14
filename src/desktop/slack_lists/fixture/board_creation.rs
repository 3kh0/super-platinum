//! Dedicated disconnected Board lane Add data and actual prepared-request ledger.
//! The runner opt-in is required only for checks and recording, not static rendering.
use super::*;

pub(crate) const SELECT_VIEW: &str = "V_BOARD_ADD_SELECT";
pub(crate) const CHECK_VIEW: &str = "V_BOARD_ADD_CHECK";

#[derive(Clone, Default)]
pub(crate) struct State {
    armed: bool,
    requests: Vec<Value>,
}

pub(crate) fn check_enabled() -> bool {
    std::env::var("SUPER_PLATINUM_SLACK_LIST_BOARD_ADD_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("slack-list-board-add" | "slack-list-board-add-narrow")
        )
}

/// No check flag or private directory is needed for the static offline picture.
pub(crate) fn install(shell: &mut ShellState, name: &str) {
    assert!(shell.core.transport.is_none() && shell.core.session.is_none());
    // Set the synthetic authenticated identity before List scope capture.
    // Short general fixture IDs deliberately cannot seed a typed Slack user.
    let viewer = "U00000001";
    shell.self_account.user_id = viewer.into();
    let workspace = shell
        .core
        .workspaces
        .get_mut("T1")
        .expect("fixture workspace");
    workspace.self_user_id = viewer.into();
    let user: super_platinum_core::slack::models::User = serde_json::from_value(json!({
        "id":viewer,"name":"Board Fixture Viewer","is_self":true,
        "profile":{"real_name":"Board Fixture Viewer","display_name":"Board Fixture Viewer"}
    }))
    .expect("fixture viewer");
    workspace.users.insert(viewer.into(), user);
    super::install(shell, name);
    let viewer = shell.self_account.user_id.clone();
    let fixture = shell.slack_lists.fixture.as_mut().unwrap();
    fixture.board_creation = State {
        armed: true,
        requests: Vec::new(),
    };
    let columns = json!([
        {"id":"C_NAME","key":"NAME","visible":true},
        {"id":"C_USER","key":"assignee","visible":true},
        {"id":"C_SELECT","key":"status","visible":true},
        {"id":"C_CHECK","key":"done","visible":true}
    ]);
    fixture.document["list_limits"] =
        json!({"over_row_maximum":false,"row_count":3,"row_count_limit":1000});
    fixture.document["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap()
        .retain(|column| {
            ["C_NAME", "C_USER", "C_SELECT", "C_CHECK"]
                .iter()
                .any(|id| column["id"] == *id)
        });
    let filters = json!([
        {"column_id":"C_NAME","key":"NAME","operator":"contains","values":["Board task","ignored second value"]},
        {"column_id":"C_USER","key":"assignee","operator":"is_me","values":["viewer","U_NOT_THE_VIEWER"]}
    ]);
    fixture.document["list_metadata"]["views"] = json!([
        {"id":SELECT_VIEW,"name":"Release lanes","type":"board","position":"1","columns":columns,"filters":filters,"sorts":[],
         "grouping":{"group_by":"status","group_by_column_id":"C_SELECT","order":[{"select":["OPT_PROGRESS"]},{"select":[]},{"select":["OPT_DONE"]}]}},
        {"id":CHECK_VIEW,"name":"Completion lanes","type":"board","position":"2","columns":columns,"filters":filters,"sorts":[],
         "grouping":{"group_by":"done","group_by_column_id":"C_CHECK","order":[{"checkbox":false},{"checkbox":true}]}}
    ]);
    fixture.records = json!({"ok":true,"records":[
        {"id":"R_BOARD_POPULATED","list_id":"F_LIST_FIXTURE","position":"5000000000","fields":[
            {"key":"NAME","text":"Board task ready"},{"key":"assignee","user":[viewer]},
            {"key":"status","select":["OPT_PROGRESS"]},{"key":"done","checkbox":false}]},
        {"id":"R_BOARD_DONE","list_id":"F_LIST_FIXTURE","position":"5000000001","fields":[
            {"key":"NAME","text":"Board task finished"},{"key":"assignee","user":[viewer]},
            {"key":"status","select":["OPT_DONE"]},{"key":"done","checkbox":true}]},
        {"id":"R_BOARD_UNASSIGNED","list_id":"F_LIST_FIXTURE","position":"5000000002","fields":[
            {"key":"NAME","text":"Board task unassigned"},{"key":"assignee","user":[viewer]},
            {"key":"status","select":[]},{"key":"done","checkbox":false}]}
    ]});
    refresh(shell);
    let view = shell.slack_lists.view.as_ref().unwrap();
    let (visit, generation) = (view.generation, view.layout.generation);
    assert!(layout::select_shell(
        shell,
        Some(SELECT_VIEW.into()),
        visit,
        generation
    ));
}

/// Called with the actual builder result immediately before the existing fixture
/// dispatcher consumes its body. Never rebuild a request for evidence.
pub(in crate::slack_lists) fn record_request(
    shell: &mut ShellState,
    request: &super_platinum_core::slack::client::PreparedRequest,
) {
    if !check_enabled() || !enabled(shell) {
        return;
    }
    let Some(fixture) = shell
        .slack_lists
        .fixture
        .as_mut()
        .filter(|f| f.board_creation.armed)
    else {
        return;
    };
    let fields = match &request.body {
        RequestBody::Form(fields) => json!(fields),
        _ => Value::Null,
    };
    fixture.board_creation.requests.push(json!({
        "method":request.method,"url":request.url,"retrySafe":request.retry_safe(),"fields":fields
    }));
}

pub(crate) fn requests(shell: &ShellState) -> Vec<Value> {
    shell
        .slack_lists
        .fixture
        .as_ref()
        .map(|f| f.board_creation.requests.clone())
        .unwrap_or_default()
}

/// Change only the simulated canonical preflight response. The mounted source
/// stays untouched so a real click must be rejected before request preparation.
pub(crate) fn change_backend(shell: &mut ShellState, changed: bool) {
    assert!(enabled(shell));
    let fixture = shell.slack_lists.fixture.as_mut().unwrap();
    fixture.document["list_metadata"]["views"][1]["filters"][0]["values"][0] = json!(if changed {
        "Canonical replacement"
    } else {
        "Board task"
    });
}

pub(crate) fn expected_fields(viewer: &str, checkbox: bool, empty: bool) -> Value {
    let lane = if checkbox {
        json!({"column_id":"C_CHECK","checkbox":false})
    } else {
        json!({"column_id":"C_SELECT","select":if empty {vec![]} else {vec!["OPT_PROGRESS"]}})
    };
    json!([
        {"column_id":"C_NAME","rich_text":[{"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":"Board task"}]}]}]},
        {"column_id":"C_USER","user":[viewer]},
        lane
    ])
}

pub(crate) fn validate_request(raw: &Value, expected: &Value) -> bool {
    let Some(pairs) = raw["fields"].as_array() else {
        return false;
    };
    let fields = pairs
        .iter()
        .map(|pair| {
            let pair = pair.as_array()?;
            if pair.len() != 2 {
                return None;
            }
            Some((pair[0].as_str()?, pair[1].as_str()?))
        })
        .collect::<Option<Vec<_>>>();
    let Some(fields) = fields else {
        return false;
    };
    let get = |name: &str| {
        fields
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| *value)
    };
    let allowed = [
        "token",
        "list_id",
        "position",
        "updated_timestamp",
        "_x_reason",
        "initial_fields",
    ];
    raw["method"] == "POST"
        && raw["retrySafe"] == false
        && raw["url"].as_str().is_some_and(|s| {
            s.split('?').next() == Some("https://example.test/api/lists.records.create")
        })
        && fields.len() == allowed.len()
        && allowed
            .iter()
            .all(|key| fields.iter().filter(|(name, _)| name == key).count() == 1)
        && get("token") == Some("fixture-not-a-token")
        && get("list_id") == Some("F_LIST_FIXTURE")
        && get("position") == Some("")
        && get("_x_reason") == Some("create-record")
        && get("updated_timestamp").is_some_and(|s| !s.is_empty())
        && get("initial_fields")
            .and_then(|s| serde_json::from_str::<Value>(s).ok())
            .as_ref()
            == Some(expected)
}

#[cfg(test)]
mod tests;
