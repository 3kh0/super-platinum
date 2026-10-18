//! New-root stages share the existing account journal and whole-file write lock.
//! Reading a row never reissues either the anchor update or Qpc root bootstrap.
use super::*;
use crate::canvas::comments::bootstrap::{BootstrapAccepted, BootstrapIntent, BootstrapRecord};
use crate::canvas::edit::annotation::{
    AnnotationObservation, AnnotationRecord, VerifiedAnnotation,
};
use serde::{Deserialize, Serialize};
mod history;
pub use history::AnnotationHistory;
use history::history_matches;
const MAX_ANNOTATION_BODY: usize = 512 * 1024;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnnotationReceipt {
    id: String,
    record: AnnotationRecord,
}
impl AnnotationReceipt {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn record(&self) -> &AnnotationRecord {
        &self.record
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnnotationStage {
    AnchorStarted,
    AnchorConfirmed,
    BootstrapStarted,
    BootstrapNotSent,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnnotationPending {
    receipt: AnnotationReceipt,
    stage: AnnotationStage,
    canonical_control: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    canonical_anchor: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    canonical_changed_controls: Vec<Vec<u8>>,
    bootstrap: Option<BootstrapRecord>,
    bootstrap_attempt: Option<String>,
}
impl AnnotationPending {
    pub fn receipt(&self) -> &AnnotationReceipt {
        &self.receipt
    }
    pub fn stage(&self) -> AnnotationStage {
        self.stage
    }
}
pub enum AnnotationBegin {
    Started(AnnotationReceipt),
    Blocked(AnnotationPending),
}
pub enum AnnotationDecision {
    RemoveLocalLockOutcomeUnknown,
}

fn encoded<T: Serialize>(v: &T) -> Result<String, AppError> {
    let s = serde_json::to_string(v)?;
    if s.len() > MAX_ANNOTATION_BODY {
        return Err(invalid());
    }
    Ok(s)
}
fn validate(p: &AnnotationPending) -> Result<(), AppError> {
    uuid::Uuid::parse_str(p.receipt.id()).map_err(|_| invalid())?;
    p.receipt.record.validate().map_err(|_| invalid())?;
    match (&p.stage, &p.canonical_control) {
        (AnnotationStage::AnchorStarted, None) => {}
        (
            AnnotationStage::AnchorConfirmed
            | AnnotationStage::BootstrapStarted
            | AnnotationStage::BootstrapNotSent,
            Some(raw),
        ) => {
            let s = crate::canvas::wire::Section::decode(raw).map_err(|_| invalid())?;
            if s.id() != p.receipt.record.control_id()
                || s.section_type().map_err(|_| invalid())? != Some(9)
            {
                return Err(invalid());
            }
            p.receipt.record.check_control(&s).map_err(|_| invalid())?;
        }
        _ => return Err(invalid()),
    }
    match (
        p.stage,
        p.receipt.record.has_parent_update(),
        p.canonical_anchor.as_deref(),
    ) {
        (AnnotationStage::AnchorStarted, _, None) => {}
        (_, false, None) => {}
        (
            AnnotationStage::AnchorConfirmed
            | AnnotationStage::BootstrapStarted
            | AnnotationStage::BootstrapNotSent,
            true,
            Some(raw),
        ) => {
            let s = crate::canvas::wire::Section::decode(raw).map_err(|_| invalid())?;
            p.receipt
                .record
                .check_anchor(&s, false)
                .map_err(|_| invalid())?;
        }
        _ => return Err(invalid()),
    }
    if p.stage == AnnotationStage::AnchorStarted {
        if !p.canonical_changed_controls.is_empty() {
            return Err(invalid());
        }
    } else {
        p.receipt
            .record
            .check_changed_controls(&p.canonical_changed_controls, false)
            .map_err(|_| invalid())?;
    }
    match (&p.stage, &p.bootstrap) {
        (AnnotationStage::AnchorStarted | AnnotationStage::AnchorConfirmed, None) => {}
        (AnnotationStage::BootstrapStarted | AnnotationStage::BootstrapNotSent, Some(record)) => {
            record.validate().map_err(|_| invalid())?;
            if record.annotation() != &p.receipt.record
                || Some(record.canonical_control()) != p.canonical_control.as_deref()
                || record.canonical_anchor() != p.canonical_anchor.as_deref()
                || record.canonical_changed_controls() != p.canonical_changed_controls
            {
                return Err(invalid());
            }
        }
        _ => return Err(invalid()),
    }
    if p.bootstrap.is_some() {
        uuid::Uuid::parse_str(p.bootstrap_attempt.as_deref().ok_or_else(invalid)?)
            .map_err(|_| invalid())?;
    } else if p.bootstrap_attempt.is_some() {
        return Err(invalid());
    }
    Ok(())
}
pub(super) fn present(conn: &Connection, s: &Scope) -> Result<bool, AppError> {
    scope_valid(s)?;
    Ok(conn.query_row("SELECT EXISTS(SELECT 1 FROM annotation_attempts WHERE workspace=?1 AND user=?2 AND file=?3)",
        params![s.workspace,s.user,s.file],|r|r.get(0))?)
}
fn read(conn: &Connection, s: &Scope) -> Result<Option<AnnotationPending>, AppError> {
    scope_valid(s)?;
    let row:Option<(String,String)>=conn.query_row(
        "SELECT id,CASE WHEN length(CAST(body AS BLOB))<=?4 THEN body ELSE NULL END FROM annotation_attempts WHERE workspace=?1 AND user=?2 AND file=?3",
        params![s.workspace,s.user,s.file,MAX_ANNOTATION_BODY as i64],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    let Some((id, body)) = row else {
        return Ok(None);
    };
    let p: AnnotationPending = serde_json::from_str(&body)?;
    validate(&p)?;
    if p.receipt.id != id || p.receipt.record.scope() != s {
        return Err(invalid());
    }
    Ok(Some(p))
}
fn replace(
    tx: &Connection,
    old: &AnnotationPending,
    new: &AnnotationPending,
) -> Result<(), AppError> {
    validate(new)?;
    let s = old.receipt.record.scope();
    let changed=tx.execute("UPDATE annotation_attempts SET body=?6 WHERE workspace=?1 AND user=?2 AND file=?3 AND id=?4 AND body=?5",
        params![s.workspace,s.user,s.file,old.receipt.id,encoded(old)?,encoded(new)?])?;
    if changed != 1 {
        return Err(invalid());
    }
    Ok(())
}
fn finish(
    tx: &Connection,
    p: &AnnotationPending,
    status: &str,
    observed: serde_json::Value,
) -> Result<(), AppError> {
    archive(tx, p, &p.receipt.id, status, observed)?;
    let s = p.receipt.record.scope();
    let body = encoded(p)?;
    if tx.execute("DELETE FROM annotation_attempts WHERE workspace=?1 AND user=?2 AND file=?3 AND id=?4 AND body=?5",
        params![s.workspace,s.user,s.file,p.receipt.id,body])?!=1 {return Err(invalid());}
    Ok(())
}
fn archive(
    tx: &Connection,
    p: &AnnotationPending,
    id: &str,
    status: &str,
    observed: serde_json::Value,
) -> Result<(), AppError> {
    history::validate_history(id, p, status, &observed)?;
    let s = p.receipt.record.scope();
    let body = encoded(p)?;
    let observed = encoded(&observed)?;
    let bytes:i64=tx.query_row("SELECT coalesce(sum(length(CAST(body AS BLOB))+length(CAST(observed AS BLOB))),0) FROM annotation_history",[],|r|r.get(0))?;
    if bytes < 0 || bytes as usize + body.len() + observed.len() > 64 * 1024 * 1024 {
        return Err(invalid());
    }
    tx.execute("INSERT INTO annotation_history(workspace,user,file,id,body,status,observed) VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![s.workspace,s.user,s.file,id,body,status,observed])?;
    Ok(())
}
impl Journal {
    /// Preserve an actual late root result after an explicit local disposition
    /// or a newer request. This insert-only evidence does not settle another
    /// attempt, remove its lock, or authorize any dispatch.
    pub fn preserve_annotation_root_result(
        &mut self,
        expected: &AnnotationPending,
        proof: &BootstrapAccepted,
    ) -> Result<bool, AppError> {
        validate(expected)?;
        if expected.stage != AnnotationStage::BootstrapStarted
            || expected.bootstrap.as_ref() != Some(proof.record())
        {
            return Err(invalid());
        }
        let id = expected.bootstrap_attempt.as_deref().ok_or_else(invalid)?;
        let observed = serde_json::to_value(proof.root())?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let s = expected.receipt.record.scope();
        let row:Option<(String,String,String)>=tx.query_row("SELECT CASE WHEN length(CAST(body AS BLOB))<=?5 THEN body ELSE NULL END,status,CASE WHEN length(CAST(observed AS BLOB))<=?5 THEN observed ELSE NULL END FROM annotation_history WHERE workspace=?1 AND user=?2 AND file=?3 AND id=?4",
            params![s.workspace,s.user,s.file,id,MAX_ANNOTATION_BODY as i64],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        if let Some((body, status, raw)) = row {
            let known: AnnotationPending = serde_json::from_str(&body)?;
            validate(&known)?;
            return Ok(known == *expected
                && status == "late_root_observed"
                && serde_json::from_str::<serde_json::Value>(&raw)? == observed);
        }
        archive(&tx, expected, id, "late_root_observed", observed)?;
        tx.commit()?;
        Ok(true)
    }
    /// Explicit continuation only for a canonically confirmed anchor whose root
    /// request has not been sent (or was definitely refused before handoff).
    /// The commit spends this admission; a lost return leaves Started locked.
    pub fn continue_annotation_bootstrap(
        &mut self,
        expected: &AnnotationPending,
        fresh: &crate::slack::api::CanvasCollabSnapshot,
        now_millis: u64,
    ) -> Result<Option<BootstrapIntent>, AppError> {
        validate(expected)?;
        if !matches!(
            expected.stage,
            AnnotationStage::AnchorConfirmed | AnnotationStage::BootstrapNotSent
        ) {
            return Ok(None);
        }
        let intent = BootstrapIntent::resume_confirmed_with_controls(
            expected.receipt.record.clone(),
            expected.canonical_control.clone().ok_or_else(invalid)?,
            expected.canonical_anchor.clone(),
            expected.canonical_changed_controls.clone(),
            fresh,
            now_millis,
        )
        .map_err(|_| invalid())?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if read(&tx, expected.receipt.record.scope())?.as_ref() != Some(expected) {
            return Ok(None);
        }
        if expected.stage == AnnotationStage::BootstrapNotSent {
            archive(
                &tx,
                expected,
                &uuid::Uuid::new_v4().to_string(),
                "bootstrap_not_sent",
                serde_json::Value::Null,
            )?;
        }
        let mut next = expected.clone();
        next.stage = AnnotationStage::BootstrapStarted;
        next.bootstrap = Some(intent.record().clone());
        next.bootstrap_attempt = Some(uuid::Uuid::new_v4().to_string());
        replace(&tx, expected, &next)?;
        tx.commit()?;
        Ok(Some(intent))
    }
    pub fn annotation_bootstrap_accepted(
        &mut self,
        expected: &AnnotationPending,
        proof: &BootstrapAccepted,
    ) -> Result<bool, AppError> {
        validate(expected)?;
        let r = &expected.receipt;
        if expected.stage != AnnotationStage::BootstrapStarted
            || expected.bootstrap.as_ref() != Some(proof.record())
        {
            return Err(invalid());
        }
        if proof.record().annotation() != r.record() {
            return Err(invalid());
        }
        let observed = serde_json::to_value(proof.root())?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let Some(p) = read(&tx, r.record.scope())? else {
            let known:Option<(String,String,String)>=tx.query_row("SELECT status,CASE WHEN length(CAST(body AS BLOB))<=?5 THEN body ELSE NULL END,CASE WHEN length(CAST(observed AS BLOB))<=?5 THEN observed ELSE NULL END FROM annotation_history WHERE id=?1 AND workspace=?2 AND user=?3 AND file=?4",
                params![r.id,r.record.scope().workspace,r.record.scope().user,r.record.scope().file,MAX_ANNOTATION_BODY as i64],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional()?;
            let Some((status, body, value)) = known else {
                return Ok(false);
            };
            let p: AnnotationPending = serde_json::from_str(&body)?;
            validate(&p)?;
            return Ok(status == "root_accepted"
                && p == *expected
                && serde_json::from_str::<serde_json::Value>(&value)? == observed);
        };
        if p != *expected
            || p.stage != AnnotationStage::BootstrapStarted
            || p.bootstrap.as_ref() != Some(proof.record())
        {
            return Ok(false);
        }
        finish(&tx, &p, "root_accepted", observed)?;
        tx.commit()?;
        Ok(true)
    }
    pub fn annotation(&self, s: &Scope) -> Result<Option<AnnotationPending>, AppError> {
        read(&self.conn, s)
    }
    pub fn begin_annotation(
        &mut self,
        record: AnnotationRecord,
    ) -> Result<AnnotationBegin, AppError> {
        record.validate().map_err(|_| invalid())?;
        let s = record.scope().clone();
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(p) = read(&tx, &s)? {
            return Ok(AnnotationBegin::Blocked(p));
        }
        let blocked:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM receipts WHERE workspace=?1 AND user=?2 AND file=?3) OR EXISTS(SELECT 1 FROM comment_receipts WHERE workspace=?1 AND user=?2 AND file=?3) OR EXISTS(SELECT 1 FROM reply_attempts WHERE workspace=?1 AND user=?2 AND file=?3)",params![s.workspace,s.user,s.file],|r|r.get(0))?;
        let total:i64=tx.query_row("SELECT (SELECT count(*) FROM receipts)+(SELECT count(*) FROM comment_receipts)+(SELECT count(*) FROM reply_attempts)+(SELECT count(*) FROM annotation_attempts)",[],|r|r.get(0))?;
        if blocked || total >= MAX_RECEIPTS {
            return Err(invalid());
        }
        let receipt = AnnotationReceipt {
            id: uuid::Uuid::new_v4().to_string(),
            record,
        };
        let pending = AnnotationPending {
            receipt: receipt.clone(),
            stage: AnnotationStage::AnchorStarted,
            canonical_control: None,
            canonical_anchor: None,
            canonical_changed_controls: vec![],
            bootstrap: None,
            bootstrap_attempt: None,
        };
        tx.execute(
            "INSERT INTO annotation_attempts(workspace,user,file,id,body) VALUES(?1,?2,?3,?4,?5)",
            params![s.workspace, s.user, s.file, receipt.id, encoded(&pending)?],
        )?;
        tx.commit()?;
        Ok(AnnotationBegin::Started(receipt))
    }
    pub fn annotation_anchor_confirmed(
        &mut self,
        r: &AnnotationReceipt,
        proof: &VerifiedAnnotation,
    ) -> Result<bool, AppError> {
        if proof.record().map_err(|_| invalid())? != r.record {
            return Err(invalid());
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let Some(p) = read(&tx, r.record.scope())? else {
            return Ok(false);
        };
        if p.receipt != *r {
            return Ok(false);
        }
        let bytes = proof.section().raw().raw().to_vec();
        let anchor = proof.anchor().map(|s| s.raw().raw().to_vec());
        if p.stage == AnnotationStage::AnchorConfirmed
            && p.canonical_control.as_ref() == Some(&bytes)
            && p.canonical_anchor == anchor
            && p.canonical_changed_controls == proof.changed_controls()
        {
            return Ok(true);
        }
        if p.stage != AnnotationStage::AnchorStarted {
            return Ok(false);
        }
        let next = AnnotationPending {
            receipt: r.clone(),
            stage: AnnotationStage::AnchorConfirmed,
            canonical_control: Some(bytes),
            canonical_anchor: anchor,
            canonical_changed_controls: proof.changed_controls().to_vec(),
            bootstrap: None,
            bootstrap_attempt: None,
        };
        replace(&tx, &p, &next)?;
        tx.commit()?;
        Ok(true)
    }
    /// Commit before the separate potentially mutating Qpc request. A restored
    /// BootstrapStarted row is an unknown result, never permission to repeat it.
    pub fn begin_annotation_bootstrap(
        &mut self,
        r: &AnnotationReceipt,
        proof: &BootstrapIntent,
    ) -> Result<bool, AppError> {
        if proof.record().annotation() != &r.record {
            return Err(invalid());
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let Some(p) = read(&tx, r.record.scope())? else {
            return Ok(false);
        };
        if p.receipt != *r
            || p.stage != AnnotationStage::AnchorConfirmed
            || p.canonical_control.as_deref() != Some(proof.record().canonical_control())
            || p.canonical_anchor.as_deref() != proof.record().canonical_anchor()
            || p.canonical_changed_controls != proof.record().canonical_changed_controls()
        {
            return Ok(false);
        }
        let mut next = p.clone();
        next.stage = AnnotationStage::BootstrapStarted;
        next.bootstrap = Some(proof.record().clone());
        next.bootstrap_attempt = Some(uuid::Uuid::new_v4().to_string());
        replace(&tx, &p, &next)?;
        tx.commit()?;
        Ok(true)
    }
    /// This is only for a definite pre-handoff refusal. An API/HTTP error after
    /// dispatch remains in its Started stage and keeps the entire file locked.
    pub fn annotation_not_sent(&mut self, expected: &AnnotationPending) -> Result<bool, AppError> {
        validate(expected)?;
        if !matches!(
            expected.stage,
            AnnotationStage::AnchorStarted | AnnotationStage::BootstrapStarted
        ) {
            return Err(invalid());
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = read(&tx, expected.receipt.record.scope())?;
        if expected.stage == AnnotationStage::BootstrapStarted {
            let mut next = expected.clone();
            next.stage = AnnotationStage::BootstrapNotSent;
            // Same exact request only: an old failure must not classify a newer
            // explicit continuation's Started record as definitely not sent.
            if current.as_ref() == Some(&next) {
                return Ok(true);
            }
            if current.as_ref() != Some(expected) {
                return Ok(false);
            }
            replace(&tx, expected, &next)?;
        } else {
            if current.is_none() {
                return history_matches(&tx, expected, "not_sent", &serde_json::Value::Null);
            }
            if current.as_ref() != Some(expected) {
                return Ok(false);
            }
            finish(&tx, expected, "not_sent", serde_json::Value::Null)?;
        }
        tx.commit()?;
        Ok(true)
    }
    /// Explicit local disposition preserves the original request, stage and fresh
    /// observed section bytes. It does not call either endpoint or certify failure.
    pub fn resolve_annotation_locally(
        &mut self,
        p: &AnnotationPending,
        observation: AnnotationObservation,
        _decision: AnnotationDecision,
    ) -> Result<bool, AppError> {
        if observation.record() != p.receipt.record() {
            return Err(invalid());
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        validate(p)?;
        let mut evidence = serde_json::json!({"anchor":observation.anchor_bytes(),"control":observation.control_bytes()});
        if let Some(prior) = observation.prior_evidence() {
            evidence["prior"] = prior;
        }
        let current = read(&tx, p.receipt.record.scope())?;
        if current.is_none() {
            return history_matches(&tx, p, "reviewed_outcome_unknown", &evidence);
        }
        if current.as_ref() != Some(p) {
            return Ok(false);
        }
        finish(&tx, p, "reviewed_outcome_unknown", evidence)?;
        tx.commit()?;
        Ok(true)
    }
}
