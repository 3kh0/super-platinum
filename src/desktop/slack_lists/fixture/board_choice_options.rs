//! Controlled disconnected Add/Color seed; no manufactured production read callback.
use super::*;
pub(crate) fn install(shell: &mut ShellState, name: &str) {
    let mode = super::super::rename::fixture::board_choice_options::case_for(name)
        .expect("Exact option alias");
    super::board_lane_order::install(shell, "slack-list-board-choice");
    let f = shell.slack_lists.fixture.as_mut().unwrap();
    f.document["title"] = json!(mode.title());
    let options = &mut f.document["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|c| c["id"] == "C_SELECT")
        .unwrap()["options"];
    options["opaque"] = json!({"keep":[1,null,true]});
    options["choices"][0]["opaque"] = json!({"rank":2,"label_hint":"Preserve raw"});
    let loaded = super::loaded(f);
    let server = loaded.document.raw.clone();
    let v = shell.slack_lists.view.as_mut().unwrap();
    v.title = loaded.document.title.clone();
    v.loaded = Some(loaded);
    v.revision = next_id();
    super::super::rebuild_rows(shell);
    if super::super::rename::fixture::board_choice_options::check_enabled() {
        super::super::rename::fixture::board_choice_options::seed(shell, server);
    }
}
