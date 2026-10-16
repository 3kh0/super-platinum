//! Named Fields review; explicit owned checker is separate from the static seed.
use super::*;
pub(crate) fn install(shell: &mut ShellState, name: &str) {
    assert!(matches!(
        name,
        "slack-list-field-visibility" | "slack-list-field-visibility-narrow"
    ));
    column_precision::seed(shell, name);
    let f = shell.slack_lists.fixture.as_mut().unwrap();
    f.document["title"] = json!("Saved Table fields");
    let dynamic = super::super::rename::fixture::visibility_check_enabled();
    f.document["list_metadata"]["views"][0]["columns"][1]["visible"] = json!(dynamic);
    if dynamic {
        f.document["list_metadata"]["schema"][2]["options"] = json!({"precision":2});
    }
    let server = f.document.clone();
    let file = serde_json::from_value(f.document.clone()).unwrap();
    assert!(super::super::open_shell(shell, file));
    refresh(shell);
    if dynamic {
        super::super::rename::fixture::seed_visibility(shell, server);
        return;
    }
    let visit = generation(shell).unwrap();
    let selection = super::super::rename::selection(shell).unwrap();
    assert!(super::super::rename::visibility::open_shell(
        shell, visit, selection
    ));
}
