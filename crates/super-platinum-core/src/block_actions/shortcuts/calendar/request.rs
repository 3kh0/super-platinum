use super::*;
fn request(
    c: &SlackClient,
    w: &WorkspaceSession,
    method: &str,
    reason: &str,
    mut fields: Vec<(&str, String)>,
) -> PreparedRequest {
    fields.push(("_x_reason", reason.into()));
    c.rest_form(w, method, fields)
}
pub fn user_info(c: &SlackClient, w: &WorkspaceSession, p: Provider) -> PreparedRequest {
    request(
        c,
        w,
        "calendar.user.info",
        "calendar-user-info-api",
        vec![("app_type", p.key().into())],
    )
}
pub fn connected_calendars(
    c: &SlackClient,
    w: &WorkspaceSession,
    p: Provider,
    a: &Account,
) -> PreparedRequest {
    request(
        c,
        w,
        "calendar.getConnectedCalendars",
        "get-connected-calendars-api",
        match a {
            Account::Modern => vec![("cal_app_type", p.wire().into())],
            Account::Legacy { user } => vec![("calapp_user_id", user.clone())],
        },
    )
}
pub fn conference_apps(c: &SlackClient, w: &WorkspaceSession) -> PreparedRequest {
    request(
        c,
        w,
        "rooms.appOptions.list",
        "rooms-app-options-list",
        vec![],
    )
}
pub fn room(
    c: &SlackClient,
    w: &WorkspaceSession,
    app: &str,
    channel: &str,
) -> Result<PreparedRequest, Error> {
    if !identifier(app) || !identifier(channel) {
        return Err(invalid());
    }
    Ok(request(
        c,
        w,
        "rooms.request",
        "rooms-request",
        vec![("app", app.into()), ("channel", channel.into())],
    ))
}
pub fn freebusy(
    c: &SlackClient,
    w: &WorkspaceSession,
    p: Provider,
    a: &Account,
    day: &str,
    minutes: u16,
    attendees: &[String],
) -> Result<PreparedRequest, Error> {
    if time::day(day).is_none()
        || !matches!(minutes, 15 | 30 | 45 | 60 | 90 | 120)
        || user_ids(&serde_json::json!(attendees)).is_err()
    {
        return Err(invalid());
    }
    let mut fields = vec![
        (
            "cal_user_id",
            match a {
                Account::Modern => String::new(),
                Account::Legacy { user } => user.clone(),
            },
        ),
        ("cal_app_type", p.wire().into()),
        ("day", day.into()),
        ("minutes", minutes.to_string()),
    ];
    if !attendees.is_empty() {
        fields.push(("attendee_user_ids", attendees.join(",")));
    }
    Ok(request(
        c,
        w,
        "calendar.freebusy",
        "time-slot-selector",
        fields,
    ))
}
pub fn create(
    c: &SlackClient,
    w: &WorkspaceSession,
    p: Provider,
    a: &Account,
    e: &Event,
) -> Result<PreparedRequest, Error> {
    if !e.valid() {
        return Err(invalid());
    }
    let mut fields = match a {
        Account::Modern => vec![("cal_app_type", p.wire().into())],
        Account::Legacy { user } => vec![
            ("calapp_user_id", user.clone()),
            ("channel", e.share_channel.clone()),
            ("thread_ts", e.share_thread.clone()),
        ],
    };
    fields.extend([
        ("end", e.end.to_string()),
        ("start", e.start.to_string()),
        ("description", e.description.clone()),
        ("meeting_url", e.meeting_url.clone()),
        ("title", e.title.clone()),
        ("calendar_id", e.calendar.clone()),
    ]);
    if !e.attendees.is_empty() {
        fields.push(("attendee_user_ids", e.attendees.join(",")));
    }
    Ok(request(
        c,
        w,
        "calendar.event.create",
        "calendar-event-create-api",
        fields,
    ))
}

pub fn share_preference(c: &SlackClient, w: &WorkspaceSession) -> PreparedRequest {
    c.rest_form(
        w,
        "users.prefs.get",
        vec![("prefs", "calendar_share_event_in_channel".into())],
    )
}
pub fn set_share_preference(c: &SlackClient, w: &WorkspaceSession, value: bool) -> PreparedRequest {
    request(
        c,
        w,
        "users.prefs.set",
        "prefs-api/setUserPrefByApi",
        vec![
            ("name", "calendar_share_event_in_channel".into()),
            ("value", value.to_string()),
        ],
    )
}
