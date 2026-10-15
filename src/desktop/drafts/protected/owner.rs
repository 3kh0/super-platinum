//! Scope checks plus typed enqueue, never a general save/reducer callback.
use super::*;
use super_platinum_core::drafts::{
    store::sync::{LocalChoice, RemoteChoice},
    sync::{ConsumeIntent, ConsumeKind, RejectedScheduleChoice, Snapshot},
};

#[derive(Clone)]
pub(crate) struct Owner {
    pub(super) scope: Scope,
    pub(super) account: Arc<super::account::Account>,
    pub(super) gate: Arc<tokio::sync::Mutex<()>>,
}
impl Owner {
    pub fn scope(&self) -> &Scope {
        &self.scope
    }
    pub async fn review(&self) -> Result<durable::Review, durable::Error> {
        self.account.initialize().await?;
        let _guard = self.gate.lock().await;
        super::account::wait(self.account.client()?.review(self.scope.clone())).await
    }
    pub async fn local_inventory(&self) -> Result<durable::LocalInventory, durable::Error> {
        self.account.initialize().await?;
        let _guard = self.gate.lock().await;
        super::account::wait(self.account.client()?.review_local(self.scope.clone())).await
    }
    pub(super) fn client(&self) -> Result<&durable::Client, durable::Error> {
        self.account.client()
    }
    pub(super) fn check(&self, review: &durable::Review) -> Result<(), durable::Error> {
        if review.scope() != &self.scope {
            return Err(durable::Error::Stale);
        }
        Ok(())
    }
    pub(super) fn check_writer(&self, writer: &durable::Writer) -> Result<(), durable::Error> {
        if writer.scope() != &self.scope {
            return Err(durable::Error::Stale);
        }
        Ok(())
    }
    pub(super) fn check_stream(&self, stream: &durable::Stream) -> Result<(), durable::Error> {
        if !stream.is_live() || self.account.team_blocked(&self.scope.key.team) {
            return Err(durable::Error::Gap);
        }
        if stream.account() != self.scope.account_id || stream.team() != self.scope.key.team {
            return Err(durable::Error::Stale);
        }
        Ok(())
    }
    pub(super) fn register(
        &self,
        expected: durable::Review,
    ) -> Result<durable::Reply<durable::Writer>, durable::Error> {
        self.check(&expected)?;
        Ok(self.client()?.register_writer(expected))
    }
    pub(super) fn checkpoint(
        &self,
        writer: durable::Writer,
        serial: u64,
        expected: durable::Review,
        local: LocalDraft,
        direct: Option<ConsumeIntent>,
    ) -> Result<durable::Reply<durable::WriteResult>, durable::Error> {
        self.check(&expected)?;
        self.check_writer(&writer)?;
        Ok(self
            .client()?
            .checkpoint(writer, serial, expected, local, direct))
    }
    pub(super) fn preserve(
        &self,
        writer: durable::Writer,
        serial: u64,
        local: LocalDraft,
    ) -> Result<durable::Reply<durable::WriteResult>, durable::Error> {
        self.check_writer(&writer)?;
        Ok(self.client()?.preserve_version(writer, serial, local))
    }
    pub(super) fn consume(
        &self,
        stream: durable::Stream,
        expected: durable::Review,
        kind: ConsumeKind,
    ) -> Result<durable::Reply<durable::Review>, durable::Error> {
        self.check_stream(&stream)?;
        self.check(&expected)?;
        Ok(self.client()?.consume(stream, expected, kind))
    }
    pub(super) fn schedule(
        &self,
        stream: durable::Stream,
        expected: durable::Review,
        raw: Snapshot,
        post_at: i64,
    ) -> Result<durable::Reply<durable::Review>, durable::Error> {
        self.check_stream(&stream)?;
        self.check(&expected)?;
        Ok(self
            .client()?
            .reviewed_schedule(stream, expected, raw, post_at))
    }
    pub(super) fn rejected_schedule(
        &self,
        stream: durable::Stream,
        expected: durable::Review,
        choice: RejectedScheduleChoice,
        now: i64,
    ) -> Result<durable::Reply<durable::Review>, durable::Error> {
        self.check_stream(&stream)?;
        self.check(&expected)?;
        Ok(self
            .client()?
            .rejected_schedule(stream, expected, choice, now))
    }
    pub(super) fn control(
        &self,
        expected: durable::Review,
        action: durable::Control,
    ) -> Result<durable::Reply<durable::Review>, durable::Error> {
        self.check(&expected)?;
        Ok(self.client()?.control(expected, action))
    }
    pub(super) fn settle(
        &self,
        expected: durable::Review,
        choice: LocalChoice,
    ) -> Result<durable::Reply<durable::Review>, durable::Error> {
        self.check(&expected)?;
        Ok(self.client()?.settle_local(expected, choice))
    }
    pub(super) fn release(
        &self,
        expected: durable::Review,
    ) -> Result<durable::Reply<durable::Release>, durable::Error> {
        self.check(&expected)?;
        Ok(self.client()?.release(expected))
    }
    pub(super) fn keep_editing(
        &self,
        expected: durable::Review,
        intent: ConsumeIntent,
    ) -> Result<durable::Reply<()>, durable::Error> {
        self.check(&expected)?;
        Ok(self.client()?.keep_editing(expected, intent))
    }
    pub(super) fn choose_baseline(
        &self,
        evidence: durable::BaselineEvidence,
        choice: RemoteChoice,
    ) -> Result<durable::Reply<durable::Review>, durable::Error> {
        if evidence.scope() != &self.scope {
            return Err(durable::Error::Stale);
        }
        Ok(self.client()?.finish_baseline(evidence, choice))
    }
}
