use super::*;

pub(super) fn review_budget(
    conn: &Connection,
    extra_count: usize,
    extra_bytes: usize,
) -> Result<()> {
    let (n,bytes):(i64,i64)=conn.query_row("SELECT count(*),COALESCE(sum(bytes),0) FROM (
SELECT length(CAST(json_extract(body,'$.content.title') AS BLOB))+length(CAST(json_extract(body,'$.content.markdown') AS BLOB))+length(CAST(json_remove(body,'$.content') AS BLOB)) AS bytes FROM conflicts
UNION ALL SELECT length(CAST(json_extract(body,'$.original.frozen.title') AS BLOB))+length(CAST(json_extract(body,'$.original.frozen.markdown') AS BLOB))+length(CAST(json_remove(body,'$.original.frozen') AS BLOB)) AS bytes FROM late_evidence)",[],|r|Ok((r.get(0)?,r.get(1)?)))?;
    let n = usize::try_from(n).map_err(|_| Fault::Invalid)?;
    let bytes = usize::try_from(bytes).map_err(|_| Fault::Invalid)?;
    // Preserve every valid v2 content budget; bounded metadata gets its own allowance.
    const LIMIT: usize = MAX_CONFLICT_BYTES + MAX_CONFLICTS * 65536;
    if n.checked_add(extra_count).is_none_or(|n| n > MAX_CONFLICTS)
        || bytes.checked_add(extra_bytes).is_none_or(|n| n > LIMIT)
    {
        return Err(Fault::Capacity);
    }
    Ok(())
}
pub(super) fn conflict_bytes(value: &Conflict) -> Result<usize> {
    Ok(value.content.bytes() + encode(&Disposition::from_conflict(value))?.len())
}
fn evidence_bytes(value: &LateResultEvidence) -> Result<usize> {
    let mut meta = serde_json::to_value(value).map_err(|_| Fault::Invalid)?;
    meta.get_mut("original")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or(Fault::Invalid)?
        .remove("frozen");
    let size = serde_json::to_vec(&meta).map_err(|_| Fault::Invalid)?.len();
    if size > 65536 {
        return Err(Fault::Invalid);
    }
    Ok(value.original.frozen.bytes() + size)
}
impl Store {
    /// Opaque result evidence only. Never changes the retired delivery classification.
    pub fn preserve_late_result(
        &mut self,
        original: &Attempt,
        expected: &RetiredAttempt,
        incoming: Outcome,
        writer: &str,
        operation: &str,
    ) -> Result<LateResultEvidence> {
        self.owner(&original.owner)?;
        self.owner(&expected.owner)?;
        let value = LateResultEvidence {
            id: operation.into(),
            owner: original.owner.clone(),
            writer: writer.into(),
            original: original.clone(),
            retired: expected.clone(),
            incoming: incoming.delivery()?,
        };
        value.validate()?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        integrity::validate(&tx, &self.account)?;
        if read::<EvidenceDisposition>(&tx, &value.owner, operation)?.is_some() {
            return Err(Fault::Retired);
        }
        if let Some(old) = read::<LateResultEvidence>(&tx, &value.owner, operation)? {
            return if old == value {
                Ok(old)
            } else {
                Err(Fault::Stale)
            };
        }
        scope(&tx, &value.owner)?.ok_or(Fault::Invalid)?;
        if read::<RetiredAttempt>(&tx, &value.owner, &expected.id)?.as_ref() != Some(expected) {
            return Err(Fault::Stale);
        }
        review_budget(&tx, 1, evidence_bytes(&value)?)?;
        insert(&tx, &value)?;
        tx.commit()?;
        Ok(value)
    }
    pub fn late_results(&self, owner: &Owner) -> Result<Vec<LateResultEvidence>> {
        self.owner(owner)?;
        self.read_snapshot(|conn| {
            Ok(all::<LateResultEvidence>(conn, &self.account)?
                .into_iter()
                .filter(|e| e.owner == *owner)
                .collect())
        })
    }
    pub fn dispose_late_result(&mut self, expected: &LateResultEvidence) -> Result<()> {
        self.owner(&expected.owner)?;
        expected.validate()?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        integrity::validate(&tx, &self.account)?;
        if read::<EvidenceDisposition>(&tx, &expected.owner, &expected.id)?.is_some() {
            return Err(Fault::Retired);
        }
        if read::<LateResultEvidence>(&tx, &expected.owner, &expected.id)?.as_ref()
            != Some(expected)
        {
            return Err(Fault::Stale);
        }
        insert(&tx, &EvidenceDisposition::from_evidence(expected))?;
        tx.execute("DELETE FROM late_evidence WHERE id=?1", [&expected.id])?;
        tx.commit()?;
        Ok(())
    }
}
