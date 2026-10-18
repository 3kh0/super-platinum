//! Record reminders reuse the existing one-attempt, original-owner HTTP boundary.
use super::*;
pub type ListReminderFailure = ListColumnRenameFailure;
impl Transport {
    pub async fn execute_list_reminder(
        &self,
        request: PreparedRequest,
        current: impl FnMut() -> bool,
    ) -> Result<Value, ListReminderFailure> {
        self.execute_list_rename_once(request, current, allowed)
            .await
    }
}
fn allowed(request: &PreparedRequest) -> bool {
    if request.method != "POST" {
        return false;
    }
    let Ok(url) = url::Url::parse(&request.url) else {
        return false;
    };
    let origin = url.scheme() == "https"
        && url.host_str().is_some_and(|h| h.ends_with(".slack.com"))
        && url.port_or_known_default() == Some(443);
    #[cfg(test)]
    let origin = origin || (url.scheme() == "http" && url.host_str() == Some("127.0.0.1"));
    if !origin
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.query_pairs().any(|(k, _)| {
            !matches!(
                k.as_ref(),
                "_x_id"
                    | "_x_csid"
                    | "_x_desktop_ia"
                    | "_x_foreground"
                    | "_x_frontend_build_type"
                    | "_x_gantry"
                    | "_x_num_retries"
                    | "_x_version_ts"
                    | "fp"
                    | "slack_route"
            )
        })
    {
        return false;
    }
    let RequestBody::Form(fields) = &request.body else {
        return false;
    };
    if fields
        .iter()
        .map(|(k, v)| k.len().saturating_add(v.len()))
        .try_fold(0usize, usize::checked_add)
        .is_none_or(|n| n > 16 * 1024)
    {
        return false;
    }
    let update = url.path() == "/api/saved.update";
    let add = url.path() == "/api/saved.add";
    let delete = url.path() == "/api/saved.delete";
    if !update && !add && !delete {
        return false;
    }
    let mut expected = vec!["token", "item_type", "item_id", "ts"];
    if !delete {
        expected.push("date_due");
    }
    if update && fields.len() == 7 {
        expected.push("mark");
    }
    expected.push("_x_reason");
    if fields.len() != expected.len() || fields.iter().zip(expected).any(|((k, _), v)| k != v) {
        return false;
    }
    let text = |i: usize| fields[i].1.as_str();
    let id = |s: &str| !s.trim().is_empty() && s.len() <= 1024 && !s.chars().any(char::is_control);
    if text(0).is_empty() || text(1) != "list_record" || !id(text(2)) || !id(text(3)) {
        return false;
    }
    if !delete
        && text(4)
            .parse::<i64>()
            .ok()
            .is_none_or(|n| n <= 0 || n > 253_402_300_799 || n.to_string() != text(4))
    {
        return false;
    }
    if update && fields.len() == 7 && !matches!(text(5), "unarchived" | "uncompleted") {
        return false;
    }
    fields.last().unwrap().1
        == if add {
            "saved-api/saveListRecord"
        } else if update {
            "saved-api/updateSavedListRecord"
        } else {
            "saved-api/removeSavedListRecord"
        }
}
#[cfg(test)]
mod tests;
