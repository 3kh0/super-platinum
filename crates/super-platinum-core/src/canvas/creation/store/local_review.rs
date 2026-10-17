use super::*;

#[derive(Clone, PartialEq, Eq)]
pub enum LocalVersionChoice {
    Canonical,
    Conflict(String),
}

impl Store {
    /// Explicit choice over the whole displayed owner inventory. This never
    /// resolves delivery uncertainty, changes cooldown, or grants send authority.
    /// `writer` is allocated once by the controller and reused after a lost reply.
    pub fn resolve_local_review(
        &mut self,
        review: &RetirementReview,
        choice: &LocalVersionChoice,
        writer: &str,
    ) -> Result<Draft> {
        self.owner(review.owner())?;
        uuid(writer)?;
        if review.conflicts().is_empty() {
            return Err(Fault::Invalid);
        }
        let expected = review
            .drafts()
            .iter()
            .find(|d| !d.archived && review.active_id() == Some(d.id.as_str()))
            .ok_or(Fault::Stale)?;
        let content = match choice {
            LocalVersionChoice::Canonical => expected.content.clone(),
            LocalVersionChoice::Conflict(id) => review
                .conflicts()
                .iter()
                .find(|c| &c.id == id)
                .ok_or(Fault::Stale)?
                .content
                .clone(),
        };
        let mut after = expected.clone();
        after.revision = next(after.revision)?;
        after.writer = writer.into();
        after.content = content;
        after.attempt = None;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        integrity::validate(&tx, &self.account)?;
        let actual = retirement::inventory(&tx, &self.account, review.owner())?;
        if actual != *review {
            // Exact immutable receipts prove our prior commit, never a second
            // application over another process's later canonical revision.
            if active(&tx, review.owner())?.as_ref() != Some(&after) {
                return Err(Fault::Stale);
            }
            for conflict in review.conflicts() {
                if read::<Disposition>(&tx, review.owner(), &conflict.id)?.as_ref()
                    != Some(&Disposition::from_conflict(conflict))
                    || read::<Conflict>(&tx, review.owner(), &conflict.id)?.is_some()
                {
                    return Err(Fault::Stale);
                }
            }
            return Ok(after);
        }
        exact_active(&tx, expected)?;
        editable(&tx, expected)?;
        // Result-evidence rows are orthogonal; they remain visible and continue
        // to block reserve until separately reviewed through their typed API.
        for conflict in review.conflicts() {
            conflict.validate()?;
            insert(&tx, &Disposition::from_conflict(conflict))?;
            tx.execute("DELETE FROM conflicts WHERE id=?1", [&conflict.id])?;
        }
        put(&tx, &after)?;
        tx.commit()?;
        Ok(after)
    }
}

#[cfg(test)]
mod tests;
