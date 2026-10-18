use super::*;
pub struct InventoryAcknowledgment {
    job: StorageJob<String, Arc<p::Loaded>>,
    reports: Vec<(u64, crate::drafts::store::terminal::Report)>,
}
impl StorageJob<String, Arc<p::Loaded>> {
    pub fn retry_inventory_storage(&self) -> Result<(), AdmissionError> {
        self.queue
            .worker
            .retry_inventory(&self.step)
            .map_err(Into::into)
    }
    pub fn observe_inventory(
        &self,
    ) -> Result<(Arc<p::Loaded>, InventoryAcknowledgment), AdmissionError> {
        let value = self
            .result()
            .ok_or(AdmissionError::Incomplete)?
            .map_err(|_| AdmissionError::Incomplete)?;
        let reports = self
            .queue
            .worker
            .observe_inventory(&self.step, None, false)?;
        Ok((
            value,
            InventoryAcknowledgment {
                job: self.clone(),
                reports,
            },
        ))
    }
}
impl InventoryAcknowledgment {
    pub fn acknowledge(self) -> Result<(), (AdmissionError, Self)> {
        match self
            .job
            .queue
            .worker
            .observe_inventory(&self.job.step, Some(&self.reports), true)
        {
            Ok(_) => Ok(()),
            Err(e) => Err((e.into(), self)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn submission_refusal_keeps_failed_read_visible_with_unchanged_original_result() {
        use p::owned_store::{FactoryIdentity, OwnedRoute};
        let route = OwnedRoute::fresh_fixture().unwrap();
        let before = route.inspect().unwrap().wait().unwrap();
        route.commit(&before).unwrap().wait().unwrap();
        let initialized = route
            .inspect()
            .unwrap()
            .wait()
            .unwrap()
            .open()
            .unwrap()
            .initialize_cold(&FactoryIdentity::default())
            .unwrap()
            .wait()
            .unwrap();
        let ops = initialized.operations();
        let scope = Scope {
            account_id: ops.account().into(),
            key: DraftKey {
                team: "T1".into(),
                channel: "C00000001".into(),
                thread: None,
            },
        };
        let fault = ops
            .fixture_fault(scope, p::test_support::FaultPoint::InventoryRead)
            .unwrap();
        fault.submit().unwrap();
        assert_eq!(fault.wait().unwrap(), JobState::Ready);
        let job = ops.inventory().unwrap();
        job.submit().unwrap();
        assert_eq!(job.wait().unwrap(), JobState::Failed(p::Error::Storage));
        let input = job.input();
        // Counter exhaustion is checked by actual try_submit_locked after the
        // ordinary retry prechecks. No public production fault API is added.
        job.queue.worker.pending.lock().unwrap().upgrade_generation = u64::MAX;
        assert_eq!(job.retry_inventory_storage(), Err(AdmissionError::Counter));
        assert_eq!(job.state(), JobState::Failed(p::Error::Storage));
        assert!(matches!(job.result(), Some(Err(p::Error::Storage))));
        assert!(Arc::ptr_eq(&input, &job.input()));
        assert!(job.observe_inventory().is_err());
    }
}
