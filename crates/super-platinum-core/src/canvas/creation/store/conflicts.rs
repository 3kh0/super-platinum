use super::*;

impl Store {
    /// Immutable version, not a per-writer replaceable slot. Retain local input on error.
    /// operation must be freshly allocated for each distinct captured version.
    pub fn preserve_conflict(
        &mut self,
        captured_base: &Draft,
        writer: &str,
        operation: &str,
        content: Content,
    ) -> Result<Conflict> {
        let owner = &captured_base.owner;
        self.owner(owner)?;
        captured_base.validate()?;
        if captured_base.archived {
            return Err(Fault::Invalid);
        }
        uuid(writer)?;
        uuid(operation)?;
        content.validate()?;
        let value = Conflict {
            id: operation.into(),
            owner: owner.clone(),
            draft_id: captured_base.id.clone(),
            base_revision: captured_base.revision,
            writer: writer.into(),
            content,
        };
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        integrity::validate(&tx, &self.account)?;
        if read::<Disposition>(&tx, owner, operation)?.is_some() {
            return Err(Fault::Retired);
        }
        if let Some(existing) = read::<Conflict>(&tx, owner, operation)? {
            return if existing == value {
                Ok(existing)
            } else {
                Err(Fault::Stale)
            };
        }
        // A captured full version remains eligible after a competing exact discard.
        // The retained scope proves stable owner custody; no canonical row is recreated.
        scope(&tx, owner)?.ok_or(Fault::Stale)?;
        if let Some(draft) = read::<Draft>(&tx, owner, &captured_base.id)? {
            if captured_base.revision > draft.revision
                || (captured_base.revision == draft.revision
                    && (captured_base.content != draft.content
                        || captured_base.writer != draft.writer))
            {
                return Err(Fault::Stale);
            }
        }
        evidence::review_budget(&tx, 1, evidence::conflict_bytes(&value)?)?;
        insert(&tx, &value)?;
        tx.commit()?;
        Ok(value)
    }
    pub fn conflicts(&self, owner: &Owner) -> Result<Vec<Conflict>> {
        self.owner(owner)?;
        self.read_snapshot(|conn| {
            Ok(all::<Conflict>(conn, &self.account)?
                .into_iter()
                .filter(|c| c.owner == *owner)
                .collect())
        })
    }
    /// Explicit user review disposition, exact immutable snapshot only.
    pub fn remove_conflict(&mut self, expected: &Conflict) -> Result<()> {
        self.owner(&expected.owner)?;
        expected.validate()?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        integrity::validate(&tx, &self.account)?;
        if read::<Disposition>(&tx, &expected.owner, &expected.id)?.is_some() {
            return Err(Fault::Retired);
        }
        if read::<Conflict>(&tx, &expected.owner, &expected.id)?.as_ref() != Some(expected) {
            return Err(Fault::Stale);
        }
        // The permanent indexed fence has no artificial lifetime count limit.
        insert(&tx, &Disposition::from_conflict(expected))?;
        tx.execute("DELETE FROM conflicts WHERE id=?1", [&expected.id])?;
        tx.commit()?;
        Ok(())
    }
}
