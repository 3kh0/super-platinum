use super::*;

/// Normalize only the visible name; Slack remains authoritative for reserved
/// names and workspace policies. The UI previews this exact submitted value.
pub fn normalize_channel_name(input: &str) -> Result<String, &'static str> {
    let mut name = String::new();
    for ch in input.trim().trim_start_matches('#').to_lowercase().chars() {
        if ch.is_whitespace() || ch == '.' {
            if !name.ends_with('-') {
                name.push('-');
            }
        } else {
            name.push(ch);
        }
    }
    if name.is_empty() {
        return Err("Enter a channel name.");
    }
    if name.chars().count() > 80 {
        return Err("Use 80 characters or fewer.");
    }
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_'))
    {
        return Err("Use letters, numbers, hyphens and underscores.");
    }
    if !name.chars().any(char::is_alphanumeric) {
        return Err("Include a letter or number.");
    }
    Ok(name)
}

pub fn channel_creation_team_ids(session: &WorkspaceSession, user: &User) -> Vec<String> {
    if session.team_id.starts_with('T') {
        return vec![session.team_id.clone()];
    }
    let mut teams = std::collections::BTreeSet::new();
    if let Some(ids) = user
        .extra
        .get("enterprise_user")
        .and_then(|v| v.get("teams"))
        .and_then(|v| v.as_array())
    {
        teams.extend(
            ids.iter()
                .filter_map(|v| v.as_str())
                .filter(|id| id.starts_with('T'))
                .map(str::to_owned),
        );
    }
    // The profile's team can be the organization ID, which cannot create a channel.
    if teams.is_empty() {
        if let Some(team) = user
            .profile
            .as_ref()
            .and_then(|p| p.team.as_ref())
            .filter(|t| t.starts_with('T'))
        {
            teams.insert(team.clone());
        }
    }
    teams.into_iter().collect()
}
pub async fn fetch_channel_creation_teams(
    transport: &Transport,
    client: &SlackClient,
    session: &WorkspaceSession,
) -> Result<Vec<Team>, Error> {
    if session.team_id.starts_with('T') {
        return Ok(vec![Team {
            id: session.team_id.clone(),
            name: Some(session.name.clone()),
            ..Default::default()
        }]);
    }
    let users = fetch_users_info(transport, client, session, vec![session.user_id.clone()]).await?;
    let user = users
        .into_iter()
        .find(|u| u.id == session.user_id)
        .ok_or_else(|| Error::Api("creation_workspace_unavailable".into()))?;
    let mut teams = Vec::new();
    for id in channel_creation_team_ids(session, &user) {
        #[derive(serde::Deserialize)]
        struct Info {
            team: Team,
        }
        let value = transport
            .execute(client.rest_form(session, "team.info", vec![("team", id.clone())]))
            .await?;
        let info: Info = decode(value, "channel creation workspace")?;
        if info.team.id == id {
            teams.push(info.team);
        }
    }
    if teams.is_empty() {
        return Err(Error::Api("creation_workspace_unavailable".into()));
    }
    Ok(teams)
}
pub fn conversation_create(
    client: &SlackClient,
    session: &WorkspaceSession,
    team: &str,
    name: &str,
    private: bool,
) -> PreparedRequest {
    client.rest_form(
        session,
        "conversations.create",
        vec![
            ("name", name.into()),
            ("is_private", private.to_string()),
            ("team_id", team.into()),
            ("validate_name", "true".into()),
        ],
    )
}
pub async fn create_conversation(
    transport: &Transport,
    client: &SlackClient,
    session: &WorkspaceSession,
    team: &str,
    name: &str,
    private: bool,
) -> Result<Channel, Error> {
    if !team.starts_with('T') {
        return Err(Error::Api("creation_workspace_unavailable".into()));
    }
    let name = normalize_channel_name(name).map_err(|_| Error::Api("invalid_name".into()))?;
    #[derive(serde::Deserialize)]
    struct Created {
        channel: Channel,
    }
    let result: Created = decode(
        transport
            .execute(conversation_create(client, session, team, &name, private))
            .await?,
        "create channel",
    )?;
    Ok(result.channel)
}
pub fn conversation_archive(
    client: &SlackClient,
    session: &WorkspaceSession,
    channel: &str,
    archive: bool,
) -> PreparedRequest {
    client.rest_form(
        session,
        if archive {
            "conversations.archive"
        } else {
            "conversations.unarchive"
        },
        vec![("channel", channel.into())],
    )
}
pub async fn archive_conversation(
    transport: &Transport,
    client: &SlackClient,
    session: &WorkspaceSession,
    channel: &str,
    archive: bool,
) -> Result<(), Error> {
    transport
        .execute(conversation_archive(client, session, channel, archive))
        .await
        .map(|_| ())
}
