use super::*;
use crate::slack_lists::BoardRecordMovePermit;

/// pJnP preserves a null AO position as an ordinary cell-only update. Both paths
/// consume the same one-use permit, so a fallback cannot issue a second request.
pub fn list_board_record_move(
    client: &SlackClient,
    session: &WorkspaceSession,
    permit: BoardRecordMovePermit,
) -> PreparedRequest {
    let intent = permit.into_intent();
    if let Some(position) = intent.desired_position() {
        let mut fields = vec![
            ("list_id", intent.list_id().into()),
            ("id", intent.record_id().into()),
            ("view_id", intent.view_id().into()),
            ("position", position.into()),
        ];
        if let Some(cells) = intent.cells() {
            fields.push(("cells", cells.to_string()));
        }
        fields.push(("_x_reason", "reorder-record".into()));
        client.rest_form(session, "lists.records.reorder", fields)
    } else {
        client.rest_form(
            session,
            "lists.cells.update",
            vec![
                ("list_id", intent.list_id().into()),
                (
                    "cells",
                    intent.cells().expect("captured cell-only move").to_string(),
                ),
                ("_x_reason", "bulk-update-cells".into()),
            ],
        )
    }
}
