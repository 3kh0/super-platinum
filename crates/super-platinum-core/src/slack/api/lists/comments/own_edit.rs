//! 4Tc8 ordinary rich branch only. No markdown/text fallback or delete action.
use super::*;
use crate::slack_lists::comments::OwnEditIntent;

pub fn list_record_comment_update(
    client: &SlackClient,
    session: &WorkspaceSession,
    intent: &OwnEditIntent,
) -> Result<PreparedRequest, Error> {
    let record = intent.record();
    record
        .validate()
        .map_err(|_| Error::Api("invalid_list_comment_edit".into()))?;
    if session.team_id != record.workspace() || session.user_id != record.user() {
        return Err(Error::Api("list_comment_edit_owner_changed".into()));
    }
    Ok(client.rest_form(
        session,
        "chat.update",
        vec![
            ("channel", record.target().channel_id().into()),
            ("ts", record.ts().into()),
            (
                "blocks",
                serde_json::to_string(record.blocks())
                    .map_err(|_| Error::Api("invalid_list_comment_edit".into()))?,
            ),
            ("skip_dlp_user_warning", "false".into()),
            ("_x_reason", "saveMessageEdit".into()),
        ],
    ))
}
