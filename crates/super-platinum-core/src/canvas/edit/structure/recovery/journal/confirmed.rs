//! Durable, successfully confirmed replacement facts. Pending receipts remain
//! inert locks and cannot create this record, even if their visible rows match.
use super::*;
use crate::canvas::edit::structure::{
    CanvasCollabSnapshot, ConfirmedReplacement, inventory::Owner, provenance,
};
use serde::{Deserialize, Serialize};

const VERSION: u8 = 1;

pub enum ReplacementRestore {
    Absent,
    /// Keep rendering the raw read, but do not grant structural write authority.
    Blocked,
    Restored(CanvasCollabSnapshot),
}

#[derive(PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u8,
    account: String,
    scope: Scope,
    operation: String,
    session: String,
    target: String,
    replacement: String,
    thread: String,
    document: String,
    evidence: Option<provenance::Evidence>,
}
impl Record {
    fn confirmed(owner: &str, token: &ConfirmedReplacement) -> Result<Self, AppError> {
        let c = token.checkpoint();
        let super::super::Action::Replace { replacement, .. } = &c.action else {
            return Err(invalid());
        };
        let s = token.snapshot();
        let result = Self {
            version: VERSION,
            account: owner.into(),
            scope: c.scope.clone(),
            operation: token.operation_id().into(),
            session: c.session.clone(),
            target: c.target.clone(),
            replacement: replacement.clone(),
            thread: c.thread.clone(),
            document: c.document.clone(),
            evidence: s
                .replacement_provenance
                .as_ref()
                .map(|p| p.evidence().clone()),
        };
        result.validate(owner, &c.scope)?;
        Ok(result)
    }
    fn validate(&self, owner: &str, scope: &Scope) -> Result<(), AppError> {
        scope_valid(scope)?;
        if self.version != VERSION
            || self.account != owner
            || &self.scope != scope
            || uuid::Uuid::parse_str(&self.operation).is_err()
            || self.session.len() != 12
            || !self.session.bytes().all(|c| c.is_ascii_hexdigit())
            || self.target == self.replacement
            || [
                &self.target,
                &self.replacement,
                &self.thread,
                &self.document,
            ]
            .iter()
            .any(|s| s.is_empty() || s.len() > 256 || s.chars().any(char::is_control))
        {
            return Err(invalid());
        }
        Ok(())
    }
    fn restore(&self, fresh: &CanvasCollabSnapshot) -> Result<CanvasCollabSnapshot, AppError> {
        let owner = Owner::capture(fresh).map_err(|_| invalid())?;
        if owner.workspace != self.scope.workspace
            || owner.user != self.scope.user
            || owner.file != self.scope.file
            || owner.thread != self.thread
            || owner.document != self.document
        {
            return Err(invalid());
        }
        provenance::restore(self.evidence.as_ref(), fresh).map_err(|_| invalid())
    }
}
fn other_pending(conn: &Connection, scope: &Scope) -> Result<bool, AppError> {
    Ok(annotations::present(conn, scope)?
        || comments::present(conn, scope)?
        || replies::present(conn, scope)?)
}
fn stored(conn: &Connection, owner: &str, scope: &Scope) -> Result<Option<Record>, AppError> {
    let row: Option<(String,String)> = conn.query_row(
        "SELECT operation,CASE WHEN length(CAST(body AS BLOB))<=?4 THEN body ELSE NULL END FROM confirmed_replacements WHERE workspace=?1 AND user=?2 AND file=?3",
        params![scope.workspace,scope.user,scope.file,MAX_BODY as i64], |r| Ok((r.get(0)?,r.get(1)?)),
    ).optional()?;
    let Some((id, body)) = row else {
        return Ok(None);
    };
    let record: Record = serde_json::from_str(&body)?;
    record.validate(owner, scope)?;
    if record.operation != id {
        return Err(invalid());
    }
    Ok(Some(record))
}
impl Journal {
    /// One transaction persists the confirmed cohort and retires only its exact
    /// receipt. Failed storage keeps both the prior record and pending lock.
    /// A lost completion response may retry this same token idempotently; it
    /// cannot overwrite a newer receipt or confirmed operation.
    pub fn complete_replacement(
        &mut self,
        receipt: &Receipt,
        token: &ConfirmedReplacement,
    ) -> Result<bool, AppError> {
        if receipt.operation_id() != token.operation_id()
            || receipt.checkpoint() != token.checkpoint()
        {
            return Ok(false);
        }
        let record = Record::confirmed(&self.account, token)?;
        let body = serde_json::to_string(&record)?;
        if body.len() > MAX_BODY {
            return Err(invalid());
        }
        let scope = receipt.checkpoint.scope();
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if other_pending(&tx, scope)? {
            return Ok(false);
        }
        let active = read(&tx, scope)?;
        let previous = stored(&tx, &self.account, scope)?;
        if active.as_ref() != Some(receipt) {
            return Ok(active.is_none() && previous.as_ref() == Some(&record));
        }
        // A confirmed operation may extend the earlier sealed cohort, never
        // silently downgrade it using an unrelated raw source.
        if previous
            .as_ref()
            .is_some_and(|p| p.evidence != token.checkpoint().provenance)
        {
            return Err(invalid());
        }
        tx.execute("INSERT INTO confirmed_replacements(workspace,user,file,operation,body) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(workspace,user,file) DO UPDATE SET operation=excluded.operation,body=excluded.body",
            params![scope.workspace,scope.user,scope.file,record.operation,body])?;
        let changed=tx.execute("DELETE FROM receipts WHERE workspace=?1 AND user=?2 AND file=?3 AND operation=?4 AND body=?5",
            params![scope.workspace,scope.user,scope.file,receipt.id,receipt.body])?;
        if changed != 1 {
            return Err(invalid());
        }
        tx.commit()?;
        Ok(true)
    }

    /// Blocking worker API. Inspect pending and confirmed state in one SQLite
    /// read transaction, then validate the actual current snapshot before minting
    /// live provenance. No action/session/dispatch is reconstructed.
    pub fn restore_replacement(
        &mut self,
        scope: &Scope,
        fresh: &CanvasCollabSnapshot,
    ) -> Result<ReplacementRestore, AppError> {
        scope_valid(scope)?;
        let owner = Owner::capture(fresh).map_err(|_| invalid())?;
        if owner.workspace != scope.workspace
            || owner.user != scope.user
            || owner.file != scope.file
        {
            return Err(invalid());
        }
        let tx = self.conn.transaction()?;
        if other_pending(&tx, scope)? || read(&tx, scope)?.is_some() {
            return Ok(ReplacementRestore::Blocked);
        }
        let Some(record) = stored(&tx, &self.account, scope)? else {
            return Ok(ReplacementRestore::Absent);
        };
        let snapshot = record.restore(fresh)?;
        tx.commit()?;
        Ok(ReplacementRestore::Restored(snapshot))
    }
}
