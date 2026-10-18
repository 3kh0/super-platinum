//! Bounded inert history projections; never reconstruct live permits.
use super::*;
pub(super) fn history_matches(
    conn: &Connection,
    expected: &AnnotationPending,
    status: &str,
    observed: &serde_json::Value,
) -> Result<bool, AppError> {
    let s = expected.receipt.record.scope();
    let row:Option<(String,String,String)>=conn.query_row(
        "SELECT body,status,observed FROM annotation_history WHERE workspace=?1 AND user=?2 AND file=?3 AND id=?4 AND length(CAST(body AS BLOB))<=?5 AND length(CAST(observed AS BLOB))<=?5",
        params![s.workspace,s.user,s.file,expected.receipt.id,MAX_ANNOTATION_BODY as i64],
        |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    let Some((body, stored_status, stored_observed)) = row else {
        return Ok(false);
    };
    let p: AnnotationPending = serde_json::from_str(&body)?;
    validate_history(
        &expected.receipt.id,
        &p,
        &stored_status,
        &serde_json::from_str(&stored_observed)?,
    )?;
    Ok(p == *expected
        && stored_status == status
        && serde_json::from_str::<serde_json::Value>(&stored_observed)? == *observed)
}

/// Inert, bounded local review evidence. Reading history never restores a send
/// permit, even when it contains the exact canonical root of a late success.
pub struct AnnotationHistory {
    pub id: String,
    pub pending: AnnotationPending,
    pub status: String,
    pub observed: serde_json::Value,
}
impl Journal {
    pub fn annotation_history(
        &self,
        scope: &Scope,
        after: Option<&str>,
    ) -> Result<Vec<AnnotationHistory>, AppError> {
        scope_valid(scope)?;
        if let Some(id) = after {
            uuid::Uuid::parse_str(id).map_err(|_| invalid())?;
        }
        let mut query=self.conn.prepare("SELECT id,CASE WHEN length(CAST(body AS BLOB))<=?5 THEN body ELSE NULL END,status,CASE WHEN length(CAST(observed AS BLOB))<=?5 THEN observed ELSE NULL END FROM annotation_history WHERE workspace=?1 AND user=?2 AND file=?3 AND (?4 IS NULL OR id>?4) ORDER BY id LIMIT 20")?;
        let rows = query.query_map(
            params![
                scope.workspace,
                scope.user,
                scope.file,
                after,
                MAX_ANNOTATION_BODY as i64
            ],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            },
        )?;
        let mut out = Vec::new();
        for row in rows {
            let (id, body, status, observed) = row?;
            uuid::Uuid::parse_str(&id).map_err(|_| invalid())?;
            let pending: AnnotationPending = serde_json::from_str(&body)?;
            validate(&pending)?;
            if pending.receipt.record.scope() != scope {
                return Err(invalid());
            }
            let observed = serde_json::from_str(&observed)?;
            validate_history(&id, &pending, &status, &observed)?;
            out.push(AnnotationHistory {
                id,
                pending,
                status,
                observed,
            });
        }
        Ok(out)
    }
}

impl AnnotationHistory {
    /// An observed root is display/navigation evidence only, never a send permit.
    pub fn root(&self) -> Option<crate::slack::models::Message> {
        if !matches!(self.status.as_str(), "root_accepted" | "late_root_observed") {
            return None;
        }
        self.pending
            .bootstrap
            .as_ref()?
            .observed_root(&self.observed)
            .ok()
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewEvidence {
    anchor: Vec<u8>,
    control: Option<Vec<u8>>,
    #[serde(default)]
    prior: Option<serde_json::Value>,
}
pub(super) fn validate_history(
    id: &str,
    p: &AnnotationPending,
    status: &str,
    observed: &serde_json::Value,
) -> Result<(), AppError> {
    validate(p)?;
    uuid::Uuid::parse_str(id).map_err(|_| invalid())?;
    match status {
        "not_sent"
            if id == p.receipt.id
                && p.stage == AnnotationStage::AnchorStarted
                && observed.is_null() => {}
        "bootstrap_not_sent"
            if p.stage == AnnotationStage::BootstrapNotSent && observed.is_null() => {}
        "root_accepted" | "late_root_observed" if p.stage == AnnotationStage::BootstrapStarted => {
            let expected_id = if status == "root_accepted" {
                p.receipt.id.as_str()
            } else {
                p.bootstrap_attempt.as_deref().ok_or_else(invalid)?
            };
            if id != expected_id {
                return Err(invalid());
            }
            p.bootstrap
                .as_ref()
                .ok_or_else(invalid)?
                .observed_root(observed)
                .map_err(|_| invalid())?;
        }
        "anchor_acknowledged"
            if id != p.receipt.id && p.stage == AnnotationStage::AnchorStarted =>
        {
            let object = observed.as_object().ok_or_else(invalid)?;
            if object.len()
                != (if p.receipt.record.has_parent_update() {
                    2
                } else {
                    1
                }) + usize::from(p.receipt.record.has_changed_controls())
            {
                return Err(invalid());
            }
            validate_ack_fields(&p.receipt.record, object)?;
        }

        "acknowledged_unconfirmed"
            if id == p.receipt.id && p.stage == AnnotationStage::AnchorStarted =>
        {
            let object = observed.as_object().ok_or_else(invalid)?;
            if object.len()
                != (if p.receipt.record.has_parent_update() {
                    4
                } else {
                    3
                }) + usize::from(p.receipt.record.has_prior_controls())
                    + usize::from(p.receipt.record.has_changed_controls())
                || !object.contains_key("anchor")
                || !object.contains_key("control")
            {
                return Err(invalid());
            }
            validate_ack_fields(&p.receipt.record, object)?;
            let mut review =
                serde_json::json!({"anchor":object["anchor"],"control":object["control"]});
            if let Some(prior) = object.get("prior") {
                review["prior"] = prior.clone();
            }
            let evidence: ReviewEvidence = serde_json::from_value(review)?;
            p.receipt
                .record
                .validate_observed_bytes(&evidence.anchor, evidence.control.as_deref())
                .map_err(|_| invalid())?;
            p.receipt
                .record
                .validate_prior_evidence(evidence.prior.as_ref())
                .map_err(|_| invalid())?;
        }
        "reviewed_outcome_unknown" if id == p.receipt.id => {
            let object = observed.as_object().ok_or_else(invalid)?;
            if object.len() != 2 + usize::from(p.receipt.record.has_prior_controls())
                || !object.contains_key("anchor")
                || !object.contains_key("control")
            {
                return Err(invalid());
            }
            let evidence: ReviewEvidence = serde_json::from_value(observed.clone())?;
            p.receipt
                .record
                .validate_observed_bytes(&evidence.anchor, evidence.control.as_deref())
                .map_err(|_| invalid())?;
            p.receipt
                .record
                .validate_prior_evidence(evidence.prior.as_ref())
                .map_err(|_| invalid())?;
        }
        _ => return Err(invalid()),
    }
    Ok(())
}

impl Journal {
    /// Explicitly release only the local lock after reviewing a changed canvas.
    /// The real ACK and unknown canonical outcome remain durable and inspectable.
    pub fn resolve_acknowledged_annotation_locally(
        &mut self,
        p: &AnnotationPending,
        observation: AnnotationObservation,
        acknowledged: crate::canvas::edit::annotation::AnnotationAcknowledgementEvidence,
        _decision: AnnotationDecision,
    ) -> Result<bool, AppError> {
        validate(p)?;
        if p.stage != AnnotationStage::AnchorStarted
            || observation.record() != p.receipt.record()
            || &acknowledged.record != p.receipt.record()
        {
            return Err(invalid());
        }
        let mut evidence = ack_fields(&acknowledged);
        evidence["anchor"] = serde_json::json!(observation.anchor_bytes());
        evidence["control"] = serde_json::json!(observation.control_bytes());
        if let Some(prior) = observation.prior_evidence() {
            evidence["prior"] = prior;
        }
        validate_history(&p.receipt.id, p, "acknowledged_unconfirmed", &evidence)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = read(&tx, p.receipt.record.scope())?;
        if current.is_none() {
            return history_matches(&tx, p, "acknowledged_unconfirmed", &evidence);
        }
        if current.as_ref() != Some(p) {
            return Ok(false);
        }
        finish(&tx, p, "acknowledged_unconfirmed", evidence)?;
        tx.commit()?;
        Ok(true)
    }
}

impl Journal {
    /// Preserve the actual ACK independently of the current attempt row. A late
    /// response cannot delete/settle a newer row; no canonical proof is restored.
    pub fn preserve_annotation_acknowledgement(
        &mut self,
        expected: &AnnotationPending,
        proof: &crate::canvas::edit::annotation::AnnotationAcknowledgementEvidence,
    ) -> Result<bool, AppError> {
        if expected.stage != AnnotationStage::AnchorStarted
            || expected.receipt.record() != &proof.record
        {
            return Err(invalid());
        }
        let observed = ack_fields(proof);
        validate_history(&proof.id, expected, "anchor_acknowledged", &observed)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let s = expected.receipt.record.scope();
        let row:Option<(String,String,String)>=tx.query_row("SELECT CASE WHEN length(CAST(body AS BLOB))<=?5 THEN body ELSE NULL END,status,CASE WHEN length(CAST(observed AS BLOB))<=?5 THEN observed ELSE NULL END FROM annotation_history WHERE workspace=?1 AND user=?2 AND file=?3 AND id=?4",
            params![s.workspace,s.user,s.file,proof.id,MAX_ANNOTATION_BODY as i64],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        if let Some((body, status, raw)) = row {
            let p: AnnotationPending = serde_json::from_str(&body)?;
            let value = serde_json::from_str::<serde_json::Value>(&raw)?;
            validate_history(&proof.id, &p, &status, &value)?;
            return Ok(p == *expected && status == "anchor_acknowledged" && value == observed);
        }
        archive(&tx, expected, &proof.id, "anchor_acknowledged", observed)?;
        tx.commit()?;
        Ok(true)
    }
}

fn ack_fields(
    proof: &crate::canvas::edit::annotation::AnnotationAcknowledgementEvidence,
) -> serde_json::Value {
    let mut value = serde_json::json!({"acknowledged_control":proof.section});
    if let Some(anchor) = &proof.anchor {
        value["acknowledged_anchor"] = serde_json::json!(anchor);
    }
    if !proof.changed_controls.is_empty() {
        value["acknowledged_changed_controls"] = serde_json::json!(proof.changed_controls);
    }
    value
}
fn validate_ack_fields(
    record: &AnnotationRecord,
    object: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), AppError> {
    let control: Vec<u8> = serde_json::from_value(
        object
            .get("acknowledged_control")
            .ok_or_else(invalid)?
            .clone(),
    )?;
    let anchor: Option<Vec<u8>> = object
        .get("acknowledged_anchor")
        .map(|value| serde_json::from_value(value.clone()))
        .transpose()?;
    let changed: Vec<Vec<u8>> = object
        .get("acknowledged_changed_controls")
        .map(|value| serde_json::from_value(value.clone()))
        .transpose()?
        .unwrap_or_default();
    if object.contains_key("acknowledged_changed_controls") != record.has_changed_controls() {
        return Err(invalid());
    }
    record
        .check_changed_controls(&changed, true)
        .map_err(|_| invalid())?;
    record
        .check_acknowledged_bytes(&control, anchor.as_deref())
        .map_err(|_| invalid())
}
