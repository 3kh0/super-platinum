//! Static controlled offline card-position fixture; no production read completion.
use super::*;
pub(crate) fn install(shell: &mut ShellState, name: &str) {
    let mode = super::super::rename::fixture::card_extended::case_for(name)
        .expect("Exact extended card fixture");
    super::board_lane_order::install(shell, "slack-list-board-lane-order");
    let f = shell.slack_lists.fixture.as_mut().unwrap();
    f.document["title"] = json!(mode.title());
    for (i, row) in f.records["records"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        row["position"] = json!(format!("{:010}", i + 1));
        row["view_positions"] = json!({"V_OTHER":"0000000009.V"});
        if mode == super::super::rename::fixture::card_extended::Case::Cross && matches!(i, 4 | 5) {
            row["fields"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|f| f["key"] == "status")
                .unwrap()["select"] = json!(["OPT_DONE"]);
        }
    }
    let loaded = super::loaded(f);
    let server = loaded.document.raw.clone();
    let v = shell.slack_lists.view.as_mut().unwrap();
    v.title = loaded.document.title.clone();
    v.loaded = Some(loaded);
    v.revision = next_id();
    super::super::rebuild_rows(shell);
    if super::super::rename::fixture::card_extended::check_enabled() {
        super::super::rename::fixture::card_extended::seed(shell, server);
    }
}
