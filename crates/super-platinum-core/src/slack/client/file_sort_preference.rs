//! Closed self preference only; generic form defaults are unchanged.
use super::*;

impl SlackClient {
    pub(in crate::slack) fn file_sort_preference_multipart(
        &self,
        workspace: &WorkspaceSession,
        value: &'static str,
    ) -> PreparedRequest {
        let mut request = self.rest_form(
            workspace,
            "users.prefs.set",
            vec![
                ("name", "search_file_sort".into()),
                ("value", value.into()),
                ("_x_reason", "prefs-api/setUserPrefByApi".into()),
            ],
        );
        request
            .headers
            .retain(|(key, _)| !key.eq_ignore_ascii_case("content-type"));
        request
    }
}
