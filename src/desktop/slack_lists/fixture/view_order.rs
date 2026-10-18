//! Named disconnected static review or explicitly gated dynamic ordering checker.
use super::*;
pub(crate) fn seed(shell: &mut ShellState, name: &str) {
    super::column_precision::seed(shell, name);
    let f = shell.slack_lists.fixture.as_mut().unwrap();
    f.document["title"] = json!("Shared view order review");
    let template = f.document["list_metadata"]["views"][0].clone();
    let mut views = Vec::new();
    for (id, label, position) in [
        ("V_ORDER_FIRST", "Planning", "0000000001"),
        ("V_ORDER_SELECTED", "Delivery", "0000000002"),
        ("V_ORDER_LAST", "Archive", "0000000003"),
    ] {
        let mut view = template.clone();
        view["id"] = json!(id);
        view["name"] = json!(label);
        view["position"] = json!(position);
        view["opaque"] = json!({"retained":id});
        views.push(view);
    }
    f.document["list_metadata"]["views"] = json!(views);
    f.document["list_metadata"]["default_view"] = json!("V_ORDER_SELECTED");
    shell.slack_lists.view.as_mut().unwrap().layout = Default::default();
    super::refresh(shell);
}
pub(crate) fn install(shell: &mut ShellState, name: &str) {
    assert!(matches!(
        name,
        "slack-list-view-order" | "slack-list-view-order-narrow"
    ));
    seed(shell, name);
    if super::super::rename::fixture::view_order::check_enabled() {
        // Explicit controlled offline setup before the first checker action.
        // Generic refresh may have left a loading flag under an obsolete read
        // owner; this is not a fabricated successful production read callback.
        assert!(super::super::rename::fixture::view_order::registry_empty(shell).unwrap());
        let loaded = super::loaded(shell.slack_lists.fixture.as_ref().unwrap());
        let server = loaded.document.raw.clone();
        let view = shell.slack_lists.view.as_mut().unwrap();
        view.title = loaded.document.title.clone();
        view.loaded = Some(loaded);
        view.loading = false;
        view.stale = false;
        view.unavailable = false;
        view.revision = next_id();
        view.layout = Default::default();
        super::super::rebuild_rows(shell);
        super::super::rename::fixture::view_order::seed(shell, server);
        super::super::rename::fixture::view_order::initial_readiness(shell).unwrap();
        return;
    }
    let visit = generation(shell).unwrap();
    let selected = super::super::rename::selection(shell).unwrap();
    assert!(super::super::rename::view_order::open_shell(
        shell,
        visit,
        selected,
        "V_ORDER_SELECTED",
        super::super::rename::view_order::Direction::Up
    ));
}
