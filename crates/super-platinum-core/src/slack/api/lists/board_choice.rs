//! Retained TC0j/HC6Z options-only update for a Board Select choice.
use super::*;
use crate::slack_lists::BoardChoicePermit;

pub fn list_board_choice(
    client: &SlackClient,
    session: &WorkspaceSession,
    permit: BoardChoicePermit,
) -> PreparedRequest {
    let intent = permit.into_intent();
    client.rest_form(
        session,
        "lists.columns.update",
        vec![
            ("list_id", intent.list_id().into()),
            ("key", intent.column_key().into()),
            ("options", intent.options().to_string()),
            ("backfill_missing_cells_with_default", "true".into()),
            ("_x_reason", "update-column".into()),
        ],
    )
}
