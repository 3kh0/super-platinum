//! Typed plain draft persistence in the existing account-owned inventory.
//! Deserialization grants neither current editor policy nor a replayable intent.
use super::*;

impl Journal {
    pub fn edit_save_plain(
        &mut self,
        expected: &EditInventory,
        seed: &OwnEditSeed,
        capture: &str,
        delta: PlainDelta,
    ) -> Result<EditInventory, AppError> {
        expected.validate()?;
        seed.validate().map_err(|_| invalid())?;
        delta.validate().map_err(|_| invalid())?;
        uuid(capture)?;
        if seed.plain_delta().is_none()
            || scope_for(seed)? != expected.scope
            || seed.ts() != expected.ts
        {
            return Err(invalid());
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut current = read(&tx, &expected.scope, &expected.ts)?;
        if current != *expected
            || current.seed.as_ref().is_some_and(|old| old != seed)
            || !current.blocks.is_empty()
            || current.capture == capture && current.plain_delta.as_ref() != Some(&delta)
        {
            return Err(invalid());
        }
        current.seed = Some(seed.clone());
        current.capture = capture.into();
        current.plain_delta = Some(delta);
        bump(&mut current)?;
        write(&tx, &current)?;
        check_bounds(&tx)?;
        tx.commit()?;
        Ok(current)
    }
}

#[cfg(test)]
mod tests;
