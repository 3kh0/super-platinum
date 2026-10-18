//! Two independent consumed forms from retained Q81I/TC0j/sr7+ callbacks.
use super::*;
use crate::slack_lists::{BoardGroupDeleteCellsPermit, BoardGroupDeleteSchemaPermit};
pub fn list_board_group_delete_schema(
    client: &SlackClient,
    session: &WorkspaceSession,
    permit: BoardGroupDeleteSchemaPermit,
) -> PreparedRequest {
    let i = permit.into_intent();
    client.rest_form(
        session,
        "lists.columns.update",
        vec![
            ("list_id", i.list_id().into()),
            ("key", i.column_key().into()),
            ("options", i.options().to_string()),
            ("backfill_missing_cells_with_default", "true".into()),
            ("_x_reason", "update-column".into()),
        ],
    )
}
pub fn list_board_group_delete_cells(
    client: &SlackClient,
    session: &WorkspaceSession,
    permit: BoardGroupDeleteCellsPermit,
) -> PreparedRequest {
    let i = permit.into_intent();
    client.rest_form(
        session,
        "lists.cells.update",
        vec![
            ("list_id", i.list_id().into()),
            (
                "cells",
                serde_json::to_string(i.cells()).expect("typed JSON cells"),
            ),
            ("_x_reason", "bulk-update-cells".into()),
        ],
    )
}
