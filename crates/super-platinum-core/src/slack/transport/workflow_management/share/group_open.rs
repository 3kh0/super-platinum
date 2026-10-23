use super::*;
use crate::workflow_share::group;

impl Transport {
    /// Selection-time MPIM open. Correlated membership is an open ACK, never
    /// message delivery. Every actual attempt retains the shared source retry
    /// evidence; retirement after dispatch cannot imply remote rollback.
    pub async fn execute_workflow_share_group_current(
        &self,
        request: &group::Request,
        current: impl Fn() -> bool,
        mut observe: impl FnMut(group::Attempt),
    ) -> Result<Value, Error> {
        if !allowed(request) {
            return Err(Error::Api(
                "workflow_share_group_not_sent_invalid_request".into(),
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
fn allowed(request: &group::Request) -> bool {
    allowed_prepared(request.prepared(), "/api/conversations.open") && request.fields_current()
}
#[cfg(test)]
mod tests;
