use super::*;
use std::collections::BTreeSet;

#[derive(Clone, PartialEq, Eq)]
pub struct RetirementReview {
    owner: Owner,
    scope: Option<Scope>,
    drafts: Vec<Draft>,
    attempts: Vec<Attempt>,
    decisions: Vec<ArchiveReceipt>,
    conflicts: Vec<Conflict>,
    evidence: Vec<LateResultEvidence>,
}
impl RetirementReview {
    pub(super) fn active_id(&self) -> Option<&str> {
        self.scope.as_ref().and_then(|s| s.active.as_deref())
    }

    pub fn owner(&self) -> &Owner {
        &self.owner
    }
    pub fn attempts(&self) -> &[Attempt] {
        &self.attempts
    }
    pub fn drafts(&self) -> &[Draft] {
        &self.drafts
    }
    pub fn conflicts(&self) -> &[Conflict] {
        &self.conflicts
    }
    pub fn late_results(&self) -> &[LateResultEvidence] {
        &self.evidence
    }
    pub fn retry_at(&self) -> u64 {
        self.scope.as_ref().map_or(0, |s| s.retry_at)
    }
    pub fn decisions(&self) -> &[ArchiveReceipt] {
        &self.decisions
    }
    fn retired(&self, a: &Attempt) -> RetiredAttempt {
        RetiredAttempt::from_attempt(
            a,
            self.decisions
                .iter()
                .find(|d| d.old_attempt == a.id)
                .cloned(),
        )
    }
}
pub(super) fn inventory(
    conn: &Connection,
    account: &str,
    owner: &Owner,
) -> Result<RetirementReview> {
    Ok(RetirementReview {
        owner: owner.clone(),
        scope: scope(conn, owner)?,
        drafts: all::<Draft>(conn, account)?
            .into_iter()
            .filter(|v| v.owner == *owner)
            .collect(),
        attempts: all::<Attempt>(conn, account)?
            .into_iter()
            .filter(|v| v.owner == *owner)
            .collect(),
        decisions: all::<ArchiveReceipt>(conn, account)?
            .into_iter()
            .filter(|v| v.owner == *owner)
            .collect(),
        conflicts: all::<Conflict>(conn, account)?
            .into_iter()
            .filter(|v| v.owner == *owner)
            .collect(),
        evidence: all::<LateResultEvidence>(conn, account)?
            .into_iter()
            .filter(|v| v.owner == *owner)
            .collect(),
    })
}
impl Store {
    pub fn retirement_review(&self, owner: &Owner) -> Result<RetirementReview> {
        self.owner(owner)?;
        self.read_snapshot(|conn| inventory(conn, &self.account, owner))
    }
    pub fn retired_attempt(&self, owner: &Owner, id: &str) -> Result<Option<RetiredAttempt>> {
        self.owner(owner)?;
        uuid(id)?;
        self.read_snapshot(|conn| read(conn, owner, id))
    }
    /// Stable keyset page. No total count and no OFFSET history scan.
    pub fn retired_attempt_page(
        &self,
        owner: &Owner,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<RetiredAttempt>> {
        self.owner(owner)?;
        if let Some(id) = after {
            uuid(id)?;
        }
        if !(1..=64).contains(&limit) {
            return Err(Fault::Invalid);
        }
        self.read_snapshot(|conn| {
            let mut stmt=conn.prepare("SELECT id,body FROM retired_attempts WHERE owner_key=?1 AND id>?2 ORDER BY id LIMIT ?3")?;
            let rows=stmt.query_map(params![owner.key(),after.unwrap_or(""),limit as i64],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?;
            rows.map(|r|{let(id,body)=r?;let value:RetiredAttempt=decode(&body,&owner.account,&id)?;
                if value.owner!=*owner{return Err(Fault::Invalid);}Ok(value)}).collect()
        })
    }
    pub fn retired_archive_receipt(
        &self,
        owner: &Owner,
        id: &str,
    ) -> Result<Option<ArchiveReceipt>> {
        self.owner(owner)?;
        uuid(id)?;
        self.read_snapshot(|conn| {
            let body: Option<String> = conn
                .query_row(
                    "SELECT body FROM retired_decisions WHERE id=?1",
                    [id],
                    |r| r.get(0),
                )
                .optional()?;
            body.map(|body| {
                let value: ArchiveReceipt = decode(&body, &owner.account, id)?;
                if value.owner != *owner {
                    return Err(Fault::Invalid);
                }
                Ok(value)
            })
            .transpose()
        })
    }
    /// Explicit local content retirement, never Slack deletion or creation.
    pub fn retire_terminal(
        &mut self,
        review: &RetirementReview,
        selected: &[RetirementSelection],
    ) -> Result<RetirementReceipt> {
        self.owner(&review.owner)?;
        if selected.is_empty() || selected.len() > MAX_ATTEMPTS {
            return Err(Fault::Invalid);
        }
        let mut ids = BTreeSet::new();
        for s in selected {
            RetirementSelection::new(&s.attempt, s.acknowledged_created.as_deref())?;
            if s.attempt.owner != review.owner
                || !review.attempts.contains(&s.attempt)
                || !ids.insert(s.attempt.id.clone())
            {
                return Err(Fault::Stale);
            }
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        integrity::validate(&tx, &self.account)?;
        let actual = inventory(&tx, &self.account, &review.owner)?;
        let retired: Vec<_> = selected
            .iter()
            .map(|s| review.retired(&s.attempt))
            .collect();
        for r in &retired {
            r.validate()?;
        }
        if actual != *review {
            let mut all_retired = true;
            for r in &retired {
                if read::<RetiredAttempt>(&tx, &r.owner, &r.id)?.as_ref() != Some(r) {
                    all_retired = false;
                    break;
                }
            }
            return if all_retired {
                Ok(RetirementReceipt {
                    newly_committed: false,
                    retired,
                })
            } else {
                Err(Fault::Stale)
            };
        }
        let draft_ids: BTreeSet<_> = selected
            .iter()
            .map(|s| s.attempt.draft_id.clone())
            .collect();
        for id in &draft_ids {
            let draft = review
                .drafts
                .iter()
                .find(|d| &d.id == id)
                .ok_or(Fault::Invalid)?;
            if draft.archived {
                // A closed graph only. Never orphan another historical attempt.
                if review
                    .attempts
                    .iter()
                    .any(|a| &a.draft_id == id && !ids.contains(&a.id))
                    || review.conflicts.iter().any(|c| &c.draft_id == id)
                    || review.evidence.iter().any(|e| &e.original.draft_id == id)
                {
                    return Err(Fault::Blocked);
                }
            } else if selected.iter().any(|s| {
                &s.attempt.draft_id == id && matches!(s.attempt.delivery, Delivery::Created { .. })
            }) {
                return Err(Fault::Blocked);
            }
        }
        // Preflight complete; all rows and replay fences now change atomically.
        for r in &retired {
            insert(&tx, r)?;
            tx.execute("DELETE FROM attempts WHERE id=?1", [&r.id])?;
        }
        for id in &draft_ids {
            let draft = review
                .drafts
                .iter()
                .find(|d| &d.id == id)
                .ok_or(Fault::Invalid)?;
            if draft.archived {
                for decision in review.decisions.iter().filter(|d| &d.old_draft == id) {
                    tx.execute(
                        "INSERT INTO retired_decisions(id,body) VALUES(?1,?2)",
                        params![decision.operation, encode(decision)?],
                    )?;
                    tx.execute("DELETE FROM decisions WHERE id=?1", [&decision.operation])?;
                }
                tx.execute("DELETE FROM drafts WHERE id=?1", [id])?;
            } else if draft.attempt.as_ref().is_some_and(|a| ids.contains(a)) {
                let mut after = draft.clone();
                after.attempt = None;
                after.decision_revision = next(after.decision_revision)?;
                put(&tx, &after)?;
            }
        }
        integrity::validate(&tx, &self.account)?;
        tx.commit()?;
        Ok(RetirementReceipt {
            newly_committed: true,
            retired,
        })
    }
}
