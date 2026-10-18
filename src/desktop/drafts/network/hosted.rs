//! Native/account/source fences and exact credential capture end at actual arm.
use super::*;
use super_platinum_core::drafts::store::protected::owned_store as o;
pub(crate) struct HostedAttempt {
    attempt: o::HostedAttempt,
    transport: Arc<Transport>,
    #[cfg(feature = "protected-native-fixture")]
    fixture: Option<Arc<crate::drafts::protected::production::hosted_native::Run>>,
}
impl Binding {
    pub(crate) fn arm_hosted(
        &self,
        completion: &o::HostedCompletion,
        fresh: o::HostedFresh,
        current: impl FnOnce() -> bool,
    ) -> Result<HostedAttempt, o::HostedExecuteError> {
        let input = completion.input();
        if input.review().scope().account_id != self.account
            || input.review().scope().key.team != self.workspace.team_id
            || !current()
        {
            return Err(o::HostedExecuteError::BeforeDispatch(
                o::AdmissionError::Stale,
            ));
        }
        #[cfg(feature = "protected-native-fixture")]
        let fixture = crate::drafts::protected::production::hosted_native::binding(
            &self.account,
            &self.workspace,
            &self.transport,
        )?;
        let attempt = completion.arm(fresh, &self.workspace)?;
        Ok(HostedAttempt {
            attempt,
            transport: self.transport.clone(),
            #[cfg(feature = "protected-native-fixture")]
            fixture,
        })
    }
}
impl HostedAttempt {
    pub(crate) async fn execute(self) -> Result<(), o::HostedExecuteError> {
        #[cfg(feature = "protected-native-fixture")]
        if let Some(fixture) = self.fixture {
            return fixture.complete(self.attempt).await;
        }
        self.attempt.execute(&self.transport).await
    }
}
