//! Prepared record-open requests only: this fixture never constructs a transport.
use super::*;
#[derive(Clone)]
pub(super) struct State {
    generations: std::collections::BTreeSet<u64>,
    valid: bool,
}
impl Default for State {
    fn default() -> Self {
        Self {
            generations: Default::default(),
            valid: true,
        }
    }
}
pub(in crate::slack_lists) fn record_open(
    shell: &mut ShellState,
    list: &str,
    record: &str,
    generation: u64,
) {
    if !enabled(shell) {
        return;
    }
    let session = WorkspaceSession {
        team_id: "T_LIST_FIXTURE".into(),
        enterprise_id: None,
        user_id: "U_LIST_FIXTURE".into(),
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "fixture-not-a-token".into(),
    };
    let request = api::lists_records_open(&shell.core.client, &session, list, record);
    let mut valid = request.method == "POST"
        && url::Url::parse(&request.url).is_ok_and(|url| url.path() == "/api/lists.records.open");
    valid &= match request.body {
        RequestBody::Form(fields) => {
            let get = |name: &str| {
                fields
                    .iter()
                    .find(|(key, _)| key == name)
                    .map(|(_, value)| value.as_str())
            };
            get("list_id") == Some(list)
                && get("id") == Some(record)
                && get("_x_reason") == Some("open-record")
                && get("view_id").is_none()
                && get("cells").is_none()
        }
        _ => false,
    };
    let state = &mut shell.slack_lists.fixture.as_mut().unwrap().record_open;
    state.valid &= valid && state.generations.insert(generation);
}
pub(crate) fn record_open_count(shell: &ShellState) -> usize {
    shell
        .slack_lists
        .fixture
        .as_ref()
        .map_or(0, |fixture| fixture.record_open.generations.len())
}
pub(crate) fn record_open_valid(shell: &ShellState) -> bool {
    shell
        .slack_lists
        .fixture
        .as_ref()
        .is_some_and(|fixture| fixture.record_open.valid)
}
