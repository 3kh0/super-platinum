use super::*;

pub(in super::super) fn migrate(tx: &Transaction<'_>, version: i64) -> Result<(), AppError> {
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS unsubmitted_drafts (
        team TEXT NOT NULL,user TEXT NOT NULL,file TEXT NOT NULL,
        token TEXT NOT NULL UNIQUE,revision INTEGER NOT NULL CHECK(revision>0),body TEXT NOT NULL,
        PRIMARY KEY(team,user,file));
        CREATE TABLE IF NOT EXISTS file_share_draft_attempts (
        attempt TEXT PRIMARY KEY,team TEXT NOT NULL,user TEXT NOT NULL,file TEXT NOT NULL,
        token TEXT NOT NULL,revision INTEGER NOT NULL CHECK(revision>0),body TEXT NOT NULL);
        ",
    )?;
    for (name, sql) in [
        (
            "file_share_linked_update",
            "CREATE TRIGGER file_share_linked_update BEFORE UPDATE ON receipts WHEN EXISTS(SELECT 1 FROM file_share_draft_attempts WHERE attempt=OLD.attempt) BEGIN SELECT RAISE(ABORT,'linked Share receipt requires guarded settlement'); END",
        ),
        (
            "file_share_linked_delete",
            "CREATE TRIGGER file_share_linked_delete BEFORE DELETE ON receipts WHEN EXISTS(SELECT 1 FROM file_share_draft_attempts WHERE attempt=OLD.attempt) BEGIN SELECT RAISE(ABORT,'linked Share receipt requires guarded retirement'); END",
        ),
    ] {
        let installed: Option<String> = tx
            .query_row(
                "SELECT sql FROM sqlite_schema WHERE type='trigger' AND name=?1",
                [name],
                |r| r.get(0),
            )
            .optional()?;
        match installed {
            Some(existing) if existing == sql => (),
            None if version < 2 => tx.execute_batch(sql)?,
            _ => return Err(invalid()),
        }
    }
    Ok(())
}
pub(super) fn get(conn: &Connection, scope: &DraftScope) -> Result<Option<Draft>, AppError> {
    if !valid_scope(scope) {
        return Err(invalid());
    }
    // Refuse oversized bodies before allocating them from SQLite.
    let row: Option<(Option<String>,String,i64)> = conn.query_row(
        "SELECT CASE WHEN length(CAST(body AS BLOB))<=?4 THEN body ELSE NULL END,token,revision FROM unsubmitted_drafts WHERE team=?1 AND user=?2 AND file=?3",
        params![scope.team,scope.user,scope.file,MAX_DRAFT_BODY as i64],
        |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)),
    ).optional()?;
    row.map(|(body, token, revision)| decode(body.ok_or_else(invalid)?, scope, &token, revision))
        .transpose()
}
pub(in super::super) fn check_current(conn: &Connection, expected: &Draft) -> Result<(), AppError> {
    encode(expected)?;
    if get(conn, &expected.scope)?.as_ref() != Some(expected) {
        return Err(invalid());
    }
    Ok(())
}
fn list(conn: &Connection, team: &str, user: &str) -> Result<Vec<Draft>, AppError> {
    if !identity(team) || !identity(user) {
        return Err(invalid());
    }
    let mut stmt=conn.prepare("SELECT file,token,revision,CASE WHEN length(CAST(body AS BLOB))<=?3 THEN body ELSE NULL END FROM unsubmitted_drafts WHERE team=?1 AND user=?2 ORDER BY file LIMIT ?4")?;
    let rows = stmt.query_map(
        params![team, user, MAX_DRAFT_BODY as i64, (MAX_DRAFTS + 1) as i64],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        },
    )?;
    let mut result = Vec::new();
    for row in rows {
        let (file, token, revision, body) = row?;
        result.push(decode(
            body.ok_or_else(invalid)?,
            &DraftScope {
                team: team.into(),
                user: user.into(),
                file,
            },
            &token,
            revision,
        )?);
    }
    if result.len() > MAX_DRAFTS {
        return Err(invalid());
    }
    Ok(result)
}
fn write(conn: &Connection, value: &Draft) -> Result<(), AppError> {
    let body = encode(value)?;
    conn.execute("INSERT INTO unsubmitted_drafts(team,user,file,token,revision,body) VALUES(?1,?2,?3,?4,?5,?6)",params![value.scope.team,value.scope.user,value.scope.file,value.token,number(value.revision)?,body])?;
    Ok(())
}
pub(super) fn remove(conn: &Connection, expected: &Draft) -> Result<(), AppError> {
    let changed=conn.execute("DELETE FROM unsubmitted_drafts WHERE team=?1 AND user=?2 AND file=?3 AND token=?4 AND revision=?5",params![expected.scope.team,expected.scope.user,expected.scope.file,expected.token,number(expected.revision)?])?;
    if changed != 1 {
        return Err(invalid());
    }
    Ok(())
}
impl Journal {
    pub fn draft(&self, scope: &DraftScope) -> Result<Option<Draft>, AppError> {
        get(&self.conn, scope)
    }
    /// Bounded raw inventory, including empty reservations. No cleanup or write.
    pub fn drafts(&self, team: &str, user: &str) -> Result<Vec<Draft>, AppError> {
        list(&self.conn, team, user)
    }
    /// Explicit Open only: exact absence/body CAS, then a new visit token.
    /// Existing content is never rebound to the supplied new source snapshot.
    pub fn claim_draft(
        &mut self,
        scope: DraftScope,
        expected: Option<&Draft>,
        initial: Content,
    ) -> Result<Claim, AppError> {
        validate_content(&scope, &initial)?;
        if !initial.empty() || expected.is_some_and(|d| d.scope != scope) {
            return Err(invalid());
        }
        if let Some(d) = expected {
            encode(d)?;
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = get(&tx, &scope)?;
        if current.as_ref() != expected {
            return Ok(Claim::Conflict(current));
        }
        if let Some(receipt) = associations::blocking(&tx, current.as_ref())? {
            return Ok(Claim::Blocked(receipt));
        }
        let count: i64 =
            tx.query_row("SELECT COUNT(*) FROM unsubmitted_drafts", [], |r| r.get(0))?;
        if count > MAX_DRAFTS as i64 {
            return Err(invalid());
        }
        if current.is_none() && count == MAX_DRAFTS as i64 {
            return Ok(Claim::Capacity);
        }
        let revision = current
            .as_ref()
            .map_or(Ok(1), |d| d.revision.checked_add(1).ok_or_else(invalid))?;
        number(revision)?;
        let content = current.as_ref().map_or(initial, |d| d.content.clone());
        let next = Draft {
            scope,
            token: uuid::Uuid::new_v4().to_string(),
            revision,
            content,
        };
        if let Some(old) = &current {
            remove(&tx, old)?;
        }
        write(&tx, &next)?;
        tx.commit()?;
        Ok(Claim::Claimed(next))
    }
    /// Captured explicit Close/Share. Never recreates a row or changes its source.
    /// Conflict/Blocked requires retaining the captured note for explicit review.
    pub fn save_draft(
        &mut self,
        expected: &Draft,
        content: Content,
    ) -> Result<Mutation<Draft>, AppError> {
        encode(expected)?;
        validate_content(&expected.scope, &content)?;
        if content.source != expected.content.source || content.title != expected.content.title {
            return Err(invalid());
        }
        let revision = expected.revision.checked_add(1).ok_or_else(invalid)?;
        number(revision)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = get(&tx, &expected.scope)?;
        if current.as_ref() != Some(expected) {
            return Ok(Mutation::Conflict(current));
        }
        if let Some(receipt) = associations::blocking(&tx, Some(expected))? {
            return Ok(Mutation::Blocked(receipt));
        }
        let next = Draft {
            revision,
            content,
            ..expected.clone()
        };
        remove(&tx, expected)?;
        write(&tx, &next)?;
        tx.commit()?;
        Ok(Mutation::Applied(next))
    }
    pub fn discard_draft(&mut self, expected: &Draft) -> Result<Mutation<()>, AppError> {
        encode(expected)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = get(&tx, &expected.scope)?;
        if current.as_ref() != Some(expected) {
            return Ok(Mutation::Conflict(current));
        }
        if let Some(receipt) = associations::blocking(&tx, Some(expected))? {
            return Ok(Mutation::Blocked(receipt));
        }
        remove(&tx, expected)?;
        tx.commit()?;
        Ok(Mutation::Applied(()))
    }
    /// Explicit startup/canceled-reservation maintenance only, never a tick.
    /// Can retire another process's empty claim. Its later save must conflict and
    /// retain its DOM/captured copy; this does not preserve uncaptured typing.
    pub fn cleanup_empty_drafts(&mut self, team: &str, user: &str) -> Result<usize, AppError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let rows = list(&tx, team, user)?; // Validate ALL rows before any deletion.
        associations::blocking(&tx, None)?;
        let empty: Vec<_> = rows.into_iter().filter(|d| d.content.empty()).collect();
        for row in &empty {
            remove(&tx, row)?;
        }
        tx.commit()?;
        Ok(empty.len())
    }
    /// Caller still owns all live source/destination/account admission fences.
    /// Only Started authorizes the existing one dispatch. No draft auto-send.
    pub fn begin_from_draft(&mut self, expected: &Draft, now: u64) -> Result<Begin, AppError> {
        encode(expected)?;
        let destination = expected.content.destination.as_ref().ok_or_else(invalid)?;
        let scope = Scope {
            team: expected.scope.team.clone(),
            user: expected.scope.user.clone(),
            file: expected.scope.file.clone(),
            destination: destination.id.clone(),
        };
        let intent = Intent::new(
            scope.file.clone(),
            scope.destination.clone(),
            (!expected.content.note.is_empty()).then(|| expected.content.note.clone()),
            None,
        )
        .map_err(|_| invalid())?;
        self.begin_checked(
            scope,
            intent,
            expected.content.title.clone(),
            destination.name.clone(),
            now,
            Some(expected),
        )
    }
}
