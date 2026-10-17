use super::*;
use crate::slack_lists::rename::text_create::entity as creation;

#[test]
fn cell_and_creation_requests_cannot_commit_to_each_others_real_owners() {
    let mut cell =
        ShellState::fixture_variant(crate::media::MediaRegistry::default(), "slack-list");
    crate::slack_lists::fixture::add_user_edits(&mut cell).unwrap();
    let visit = cell.slack_lists.view.as_ref().unwrap().generation;
    super::super::begin_shell(&mut cell, "R_LIST_000", "C_EDIT_USER", visit).unwrap();
    let generation = cell
        .slack_lists
        .view
        .as_ref()
        .unwrap()
        .editor
        .as_ref()
        .unwrap()
        .generation;
    let cell_request = begin(&mut cell, generation, "Alice".into()).unwrap();
    let mut create = ShellState::fixture_variant(
        crate::media::MediaRegistry::default(),
        "slack-list-people-field-create",
    );
    let stamp = creation::Stamp::capture(&create).unwrap();
    let create_request = creation::begin(&create, stamp, "".into()).unwrap();
    let before_create = creation::presentation(&create).unwrap();
    let before_cell = super::super::presentation(&cell).unwrap();
    assert!(!cell_request.current(&create));
    assert!(!create_request.current(&cell));
    finish(
        &mut create,
        &cell_request,
        Err(Error::RateLimited {
            retry_after_secs: Some(60),
        }),
    );
    creation::finish(
        &cell,
        &create_request,
        Err(Error::RateLimited {
            retry_after_secs: Some(60),
        }),
    );
    assert!(creation::presentation(&create).unwrap() == before_create);
    assert!(super::super::presentation(&cell).unwrap() == before_cell);
    assert!(crate::slack_lists::rename::fixture::close_people_fixture(
        &create
    ));
    super::super::cancel_shell(&mut cell, generation);
    assert!(cell.slack_lists.view.as_ref().unwrap().editor.is_none());
}
