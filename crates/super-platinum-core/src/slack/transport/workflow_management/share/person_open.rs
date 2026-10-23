use super::*;

impl Transport {
    /// Selection-time DM open, not a message send. Per-attempt ACK requires
    /// actual returned D/peer correlation. Source generic retry policy and the
    /// existing explicit API200 rate-limit adaptation are shared with Share.
    pub async fn execute_workflow_share_person_current(
        &self,
        request: &person::Request,
        current: impl Fn() -> bool,
        mut observe: impl FnMut(person::Attempt),
    ) -> Result<Value, Error> {
        if !allowed(request) {
            return Err(Error::Api(
                "workflow_share_person_not_sent_invalid_request".into(),
            ));
        }
        self.share_wire_execute(
            request.prepared(),
            current,
            &mut observe,
            Duration::from_millis(100),
            |raw| request.open().decode(raw.clone()).is_ok(),
        )
        .await
    }
}
fn allowed(request: &person::Request) -> bool {
    allowed_prepared(request.prepared(), "/api/conversations.open") && request.fields_current()
}

#[cfg(test)]
mod tests;
