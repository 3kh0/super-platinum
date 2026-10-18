//! Explicit accepted-record removal; never reconstruct a native clear token.
use super::*;
impl Composer {
    pub(crate) async fn retire_reviewed(
        &self,
        editor: &editor::Editor,
        expected: &Snapshot,
        id: &str,
    ) -> Result<(), durable::Error> {
        if !std::ptr::eq(self, editor.hosted.as_ref()) {
            return Err(durable::Error::Stale);
        }
        let _gate = self.gate.lock().await;
        let route = editor
            .owner
            .account
            .owned
            .as_ref()
            .ok_or(durable::Error::Unsupported)?;
        let job = {
            let mut s = self.state.lock().map_err(|_| durable::Error::Storage)?;
            if !s.loaded
                || s.save.is_some()
                || !s.historical.is_empty()
                || s.retirement.is_some()
                || (expected.loaded && expected.version != s.version)
                || expected.selection != s.selection
                || s.flight.as_ref().is_some_and(|f| !f.acknowledged())
            {
                return Err(durable::Error::Stale);
            }
            let receipt = s
                .completed
                .iter()
                .find(|r| r.record().id() == id)
                .ok_or(durable::Error::Stale)?;
            let reviewed = expected
                .completed
                .iter()
                .find(|r| r.record().id() == id)
                .ok_or(durable::Error::Stale)?;
            if serde_json::to_value(receipt).map_err(|_| durable::Error::Storage)?
                != serde_json::to_value(reviewed).map_err(|_| durable::Error::Storage)?
            {
                return Err(durable::Error::Stale);
            }
            let input = o::HostedRetirement::new(receipt.clone(), s.current.clone())?;
            let job = Held::new(route.operations.retire_hosted(input));
            s.retirement_operations = Some(route.operations.clone());
            s.retirement_retry.admission(&job);
            s.retirement = Some(job.clone());
            job
        };
        self.retired_result(job).await
    }
    pub(crate) async fn retry_retirement(&self) -> Result<(), durable::Error> {
        let _gate = self.gate.lock().await;
        let job = {
            let mut s = self.state.lock().map_err(|_| durable::Error::Storage)?;
            let original = s.retirement.clone().ok_or(durable::Error::Pending)?;
            if !s.retirement_retry.available(Some(&original)) {
                return Err(durable::Error::Pending);
            }
            let operations = s
                .retirement_operations
                .clone()
                .ok_or(durable::Error::Pending)?;
            let job = s
                .retirement_retry
                .readmit(&original, |input| operations.retire_hosted(input));
            s.retirement = Some(job.clone());
            job
        };
        if job
            .job()
            .is_ok_and(|j| j.state() == o::JobState::Failed(durable::Error::Storage))
        {
            job.job()
                .map_err(ui)?
                .retry_retirement_storage()
                .map_err(ui)?;
        }
        self.retired_result(job).await
    }
    async fn retired_result(
        &self,
        job: Arc<Held<o::HostedRetirement, o::HostedRetired>>,
    ) -> Result<(), durable::Error> {
        // Initial admission/submit refusal has no durable outcome. Keep it
        // reachable separately from Storage failure; never create a new job
        // once this original input has been admitted to a real worker slot.
        if let Err(error) = job.submit() {
            let mut s = self.state.lock().map_err(|_| durable::Error::Storage)?;
            if s.retirement.as_ref().is_some_and(|j| Arc::ptr_eq(j, &job)) {
                let _ = s.retirement_retry.submission(Err(error));
            }
            return Err(ui(error));
        }
        {
            let mut s = self.state.lock().map_err(|_| durable::Error::Storage)?;
            if !s.retirement.as_ref().is_some_and(|j| Arc::ptr_eq(j, &job)) {
                return Err(durable::Error::Stale);
            }
            s.retirement_retry.submission(Ok(())).map_err(ui)?;
        }
        job.wait().await.map_err(ui)?;
        let value = job
            .job()
            .map_err(ui)?
            .result()
            .ok_or(durable::Error::Pending)??;
        let id = job
            .job()
            .map_err(ui)?
            .input()
            .receipt()
            .record()
            .id()
            .to_owned();
        let version = {
            let s = self.state.lock().map_err(|_| durable::Error::Storage)?;
            if !s.retirement.as_ref().is_some_and(|j| Arc::ptr_eq(j, &job)) {
                return Err(durable::Error::Stale);
            }
            s.version
                .checked_add(1)
                .ok_or(durable::Error::Unsupported)?
        };
        let (_, acknowledgment) = job.job().map_err(ui)?.observe_retirement().map_err(ui)?;
        // The actual value stays in the retained original job throughout ack.
        acknowledgment
            .acknowledge()
            .map_err(|(error, _)| ui(error))?;
        let mut s = self.state.lock().map_err(|_| durable::Error::Storage)?;
        if !s.retirement.as_ref().is_some_and(|j| Arc::ptr_eq(j, &job)) {
            return Err(durable::Error::Stale);
        }

        s.current = value.remaining().cloned();
        s.selection = s
            .current
            .as_ref()
            .map(|d| Selection::from_input(d.input()))
            .or_else(|| {
                value
                    .claims()
                    .first()
                    .map(|r| Selection::from_input(r.input()))
            })
            .or_else(|| {
                value
                    .completed()
                    .first()
                    .map(|r| Selection::from_input(r.record().input()))
            })
            .unwrap_or_default();
        s.completed = value.completed().to_vec();
        s.historical = value.claims().to_vec();
        if !s.completed.iter().any(|r| r.record().id() == id)
            && s.flight.as_ref().is_some_and(|f| f.retired_by(&id))
        {
            s.flight = None;
        }
        s.retirement = None;
        s.retirement_operations = None;
        s.retirement_retry.submission(Ok(())).map_err(ui)?;
        s.version = version;
        s.persisted = version;
        Ok(())
    }
}
