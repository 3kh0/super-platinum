//! Source VYp1/qGAm/2RQr via uGgu: standard apiCall, snake-case args.
use super::{CalendarStatus, Fault, identity};
use crate::config::WorkspaceSession;
use crate::slack::client::{PreparedRequest, SlackClient};

pub fn user_status(client: &SlackClient, workspace: &WorkspaceSession) -> PreparedRequest {
    client.rest_form(workspace, "calendar.user.status", vec![])
}
pub fn event_by_link(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    status: &CalendarStatus,
    link: &str,
) -> Result<PreparedRequest, Fault> {
    if !status.connected() {
        return Err(Fault::Unresolved);
    }
    if !identity(link) {
        return Err(Fault::InvalidIdentity);
    }
    Ok(client.rest_form(
        workspace,
        "calendar.event.getByEventLinkId",
        vec![("event_link_id", link.into())],
    ))
}
pub fn window(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    status: &CalendarStatus,
    selected_app: &str,
    days: &[String],
) -> Result<PreparedRequest, Fault> {
    if !status.connected() || !identity(selected_app) {
        return Err(Fault::Unresolved);
    }
    if days.is_empty() || days.len() > 366 || !days.iter().all(|d| valid_day(d)) {
        return Err(Fault::InvalidIdentity);
    }
    let rows: Vec<_> = days
        .iter()
        .map(|day| serde_json::json!({"day":day}))
        .collect();
    Ok(client.rest_form(
        workspace,
        "calendar.event.listByDateRange",
        vec![
            ("cal_app_type", selected_app.into()),
            ("utc_days", "true".into()),
            (
                "days",
                serde_json::to_string(&rows).map_err(|_| Fault::InvalidIdentity)?,
            ),
        ],
    ))
}
pub(super) fn valid_day(day: &str) -> bool {
    day.len() == 10
        && chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d")
            .is_ok_and(|date| date.format("%Y-%m-%d").to_string() == day)
}

/// kixH's inclusive UTC yesterday through today+3; caller supplies captured now.
pub fn initial_days(today: chrono::NaiveDate) -> Result<Vec<String>, Fault> {
    (-1..=3)
        .map(|offset| {
            today
                .checked_add_signed(chrono::Duration::days(offset))
                .map(|day| day.format("%Y-%m-%d").to_string())
                .ok_or(Fault::InvalidIdentity)
        })
        .collect()
}
