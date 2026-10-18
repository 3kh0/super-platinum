use super::*;
impl Journal {
    /// Every native edit has a stable capture UUID before background persistence.
    /// A stale predecessor creates another visible version; it never overwrites.
    pub fn save_reply_draft(
        &mut self,
        scope: &Scope,
        root: &str,
        expected: Option<&ReplyDraft>,
        capture: &str,
        text: &str,
    ) -> Result<ReplySave, AppError> {
        owner(scope, root)?;
        uuid(capture)?;
        if !valid_text(text) {
            return Err(invalid());
        }
        if let Some(e) = expected {
            draft_valid(e)?
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let inventory = read(&tx, scope, root)?;
        let used: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM reply_captures WHERE id=?1)",
            [capture],
            |r| r.get(0),
        )?;
        if used {
            return match inventory.drafts.iter().find(|d| d.capture == capture) {
                Some(value) if value.text == text => Ok(ReplySave::Stored(value.clone())),
                Some(_) => Err(invalid()),
                None => Ok(ReplySave::Retired),
            };
        }
        let matching = expected.and_then(|e| inventory.drafts.iter().find(|d| *d == e));
        let value = if let Some(old) = matching {
            ReplyDraft {
                id: old.id.clone(),
                revision: old.revision.checked_add(1).ok_or_else(invalid)?,
                capture: capture.into(),
                text: text.into(),
            }
        } else {
            if inventory.drafts.len() >= MAX_VERSIONS {
                return Err(invalid());
            }
            ReplyDraft {
                id: uuid::Uuid::new_v4().to_string(),
                revision: 1,
                capture: capture.into(),
                text: text.into(),
            }
        };
        let body = encoded(&value)?;
        tx.execute("INSERT INTO reply_captures(id) VALUES(?1)", [capture])?;
        tx.execute("INSERT INTO reply_drafts(workspace,user,file,root,id,body) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(id) DO UPDATE SET body=excluded.body",params![scope.workspace,scope.user,scope.file,root,value.id,body])?;
        advance(&tx)?;
        tx.commit()?;
        Ok(ReplySave::Stored(value))
    }
    /// Exact complete inventory review. Disposed capture IDs remain indexed so a
    /// delayed retry cannot resurrect a version the user explicitly reviewed.
    pub fn choose_reply_version(
        &mut self,
        expected: &ReplyInventory,
        chosen: &ReplyDraft,
    ) -> Result<Option<ReplyDraft>, AppError> {
        if !expected.drafts.contains(chosen) || expected.attempt.is_some() {
            return Err(invalid());
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if read(&tx, &expected.scope, &expected.root)? != *expected {
            return Ok(None);
        }
        tx.execute("DELETE FROM reply_drafts WHERE workspace=?1 AND user=?2 AND file=?3 AND root=?4 AND id<>?5",params![expected.scope.workspace,expected.scope.user,expected.scope.file,expected.root,chosen.id])?;
        advance(&tx)?;
        tx.commit()?;
        Ok(Some(chosen.clone()))
    }
    pub fn discard_reply_draft(&mut self, expected: &ReplyInventory) -> Result<bool, AppError> {
        if expected.attempt.is_some() {
            return Err(invalid());
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if read(&tx, &expected.scope, &expected.root)? != *expected {
            return Ok(false);
        }
        tx.execute(
            "DELETE FROM reply_drafts WHERE workspace=?1 AND user=?2 AND file=?3 AND root=?4",
            params![
                expected.scope.workspace,
                expected.scope.user,
                expected.scope.file,
                expected.root
            ],
        )?;
        advance(&tx)?;
        tx.commit()?;
        Ok(true)
    }
}
