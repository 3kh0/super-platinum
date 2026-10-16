use super::*;
fn agrees(draft: &Draft, receipt: &Receipt) -> bool {
    draft.scope.team == receipt.scope.team
        && draft.scope.user == receipt.scope.user
        && draft.scope.file == receipt.scope.file
        && draft.content.title == receipt.file_title
        && draft.content.note.as_slice() == receipt.intent.note().unwrap_or(&[])
        && draft.content.destination.as_ref().is_some_and(|d| {
            d.id == receipt.scope.destination && d.name == receipt.destination_name
        })
        && Intent::new(
            draft.scope.file.clone(),
            receipt.scope.destination.clone(),
            (!draft.content.note.is_empty()).then(|| draft.content.note.clone()),
            None,
        )
        .is_ok_and(|intent| intent == receipt.intent)
}
fn get(conn: &Connection, receipt: &Receipt) -> Result<Option<Draft>, AppError> {
    let raw:Option<(String,String,String,String,i64,Option<String>)>=conn.query_row("SELECT team,user,file,token,revision,CASE WHEN length(CAST(body AS BLOB))<=?2 THEN body ELSE NULL END FROM file_share_draft_attempts WHERE attempt=?1",params![receipt.attempt_id,MAX_DRAFT_BODY as i64],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional()?;
    raw.map(|(team, user, file, token, revision, body)| {
        let draft = decode(
            body.ok_or_else(invalid)?,
            &DraftScope { team, user, file },
            &token,
            revision,
        )?;
        if !agrees(&draft, receipt) {
            return Err(invalid());
        }
        Ok(draft)
    })
    .transpose()
}
pub(in super::super) fn associate(
    tx: &Transaction<'_>,
    draft: &Draft,
    receipt: &Receipt,
) -> Result<(), AppError> {
    if !agrees(draft, receipt) {
        return Err(invalid());
    }
    let body = encode(draft)?;
    tx.execute("INSERT INTO file_share_draft_attempts(attempt,team,user,file,token,revision,body) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![receipt.attempt_id,draft.scope.team,draft.scope.user,draft.scope.file,draft.token,number(draft.revision)?,body])?;
    Ok(())
}
pub(in super::super) fn settled(
    tx: &Transaction<'_>,
    receipt: &Receipt,
    expected: Option<&Draft>,
) -> Result<(), AppError> {
    let Some(expected) = expected else {
        return Ok(());
    };
    if !matches!(
        receipt.state(),
        State::Server(FileShareDisposition::Accepted { .. })
    ) {
        // A late terminal NotSent can be pruned by save. Never restore an orphan.
        if super::super::get(tx, &receipt.scope)?
            .as_ref()
            .is_some_and(|r| r.attempt_id == receipt.attempt_id)
        {
            associate(tx, expected, receipt)?;
        }
        return Ok(());
    }
    // get validates the current indexed scope/source/token/body even if replaced.
    // Equality includes immutable source, note, destination, title and revision.
    if storage::get(tx, &expected.scope)?.as_ref() == Some(expected) {
        storage::remove(tx, expected)?;
    }
    Ok(())
}
pub(in super::super) fn retire_association(
    tx: &Transaction<'_>,
    receipt: &Receipt,
) -> Result<(), AppError> {
    take_association(tx, receipt)?;
    Ok(())
}
pub(in super::super) fn take_association(
    tx: &Transaction<'_>,
    receipt: &Receipt,
) -> Result<Option<Draft>, AppError> {
    let linked = get(tx, receipt)?;
    if linked.is_some() {
        let deleted = tx.execute(
            "DELETE FROM file_share_draft_attempts WHERE attempt=?1",
            [&receipt.attempt_id],
        )?;
        if deleted != 1 {
            return Err(invalid());
        }
    }
    Ok(linked)
}

/// Validate the bounded entire association inventory, so a malformed indexed
/// scope cannot hide a link from a same-file lookup. Retain only one blocker.
pub(in super::super) fn blocking(
    conn: &Connection,
    expected: Option<&Draft>,
) -> Result<Option<Receipt>, AppError> {
    let bound = MAX_UNRESOLVED + MAX_TERMINAL;
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM file_share_draft_attempts", [], |r| {
        r.get(0)
    })?;
    if count < 0 || count as usize > bound {
        return Err(invalid());
    }
    let mut stmt =
        conn.prepare("SELECT attempt FROM file_share_draft_attempts ORDER BY attempt LIMIT ?1")?;
    let attempts = stmt.query_map([(bound + 1) as i64], |r| r.get::<_, String>(0))?;
    let mut blocker = None;
    let mut seen = 0;
    for attempt in attempts {
        seen += 1;
        if seen > bound {
            return Err(invalid());
        }
        let attempt = attempt?;
        let scope: Scope = conn.query_row(
            "SELECT team,user,file,destination FROM receipts WHERE attempt=?1",
            [&attempt],
            |r| {
                Ok(Scope {
                    team: r.get(0)?,
                    user: r.get(1)?,
                    file: r.get(2)?,
                    destination: r.get(3)?,
                })
            },
        )?;
        let receipt = super::super::get(conn, &scope)?.ok_or_else(invalid)?;
        if receipt.attempt_id != attempt {
            return Err(invalid());
        }
        let linked = get(conn, &receipt)?.ok_or_else(invalid)?;
        if expected == Some(&linked) && receipt.unresolved() {
            blocker = Some(receipt);
        }
    }
    Ok(blocker)
}
