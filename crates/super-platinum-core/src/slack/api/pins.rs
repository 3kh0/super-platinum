use super::*;
use crate::pins::PinsPage;

pub fn pins_list(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel: &str,
) -> PreparedRequest {
    client.rest_form(workspace, "pins.list", vec![("channel", channel.into())])
}
pub fn pins_change(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel: &str,
    ts: &str,
    pinned: bool,
) -> PreparedRequest {
    client.rest_form(
        workspace,
        if pinned { "pins.add" } else { "pins.remove" },
        vec![("channel", channel.into()), ("timestamp", ts.into())],
    )
}
pub async fn fetch_pins(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel: &str,
) -> Result<PinsPage, Error> {
    decode(
        transport
            .execute(pins_list(client, workspace, channel))
            .await?,
        "pins",
    )
}
pub async fn change_pin(
    transport: &Transport,
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel: &str,
    ts: &str,
    pinned: bool,
) -> Result<(), Error> {
    match transport
        .execute(pins_change(client, workspace, channel, ts, pinned))
        .await
    {
        Ok(_) => Ok(()),
        Err(Error::Api(code))
            if (pinned && code == "already_pinned") || (!pinned && code == "not_pinned") =>
        {
            Ok(())
        }
        Err(error) => Err(error),
    }
}
