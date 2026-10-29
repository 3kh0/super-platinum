//! Closed final-SDK recipe; the legacy Profile grammar remains unchanged.
use super::*;

impl Transport {
    pub async fn execute_profile_long_text_member_search_current(
        &self,
        req: PreparedRequest,
        current: impl Fn() -> bool,
    ) -> Result<ProfileMemberSearchResponse, Error> {
        if !allowed(&req) {
            return Err(Error::Api(
                "invalid_profile_long_text_member_search_request".into(),
            ));
        }
        self.execute_profile_member_read_admitted(req, current)
            .await
    }
}

fn allowed(req: &PreparedRequest) -> bool {
    let mut legacy = req.clone();
    let RequestBody::Json(body) = &mut legacy.body else {
        return false;
    };
    let Some(body) = body.as_object_mut() else {
        return false;
    };
    if body.remove("include_profile_only_users") != Some(Value::Bool(true)) {
        return false;
    }
    // Structural grammar only. Owner/expiry stays with factory/caller; fixture
    // auth/header equality stays with its separately closed exact validator.
    super::profile_member_search::allowed(&legacy)
}

#[cfg(test)]
mod tests;
