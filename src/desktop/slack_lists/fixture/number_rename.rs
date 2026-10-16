//! Named disconnected name review; dynamic seam requires the exact separate checker gate.
use super::*;
pub(crate) fn install(shell: &mut ShellState, name: &str) {
    assert!(matches!(
        name,
        "slack-list-number-rename" | "slack-list-number-rename-narrow"
    ));
    column_precision::seed(shell, name);
    let f = shell.slack_lists.fixture.as_mut().unwrap();
    f.document["title"] = json!("Number field name review");
    f.document["list_metadata"]["schema"][2]["options"]["precision"] = json!(2);
    let server = f.document.clone();
    let file = serde_json::from_value(server.clone()).unwrap();
    assert!(super::super::open_shell(shell, file));
    refresh(shell);
    if super::super::rename::fixture::number_rename_check_enabled() {
        super::super::rename::fixture::seed_number_rename(shell, server);
    } else {
        let visit = generation(shell).unwrap();
        assert!(super::super::rename::open_shell(shell, "C_NUMBER", visit));
    }
}
