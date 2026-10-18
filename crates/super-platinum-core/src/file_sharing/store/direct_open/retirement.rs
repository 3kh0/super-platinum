//! Retire only a completed original workflow, inside accepted Share settlement.
use super::*;

pub(in crate::file_sharing::store) fn accepted(
    tx: &Transaction<'_>,
    linked: &drafts::Draft,
    receipt: &Receipt,
) -> Result<(), AppError> {
    if !matches!(
        receipt.state(),
        State::Server(FileShareDisposition::Accepted { .. })
    ) {
        return Err(invalid());
    }
    // A read-derived resolved destination is insufficient by itself. The actual
    // accepted receipt must still have its validated full draft association.
    let Some(destination) = &linked.content.destination else {
        return Err(invalid());
    };
    if destination.id != receipt.scope.destination {
        return Err(invalid());
    }
    if !id(&destination.id, b'D') {
        return Ok(());
    }
    let mut stmt = tx.prepare(
        "SELECT peer FROM file_share_direct_opens WHERE team=?1 AND user=?2 ORDER BY peer LIMIT ?3",
    )?;
    let peers = stmt
        .query_map(
            params![linked.scope.team, linked.scope.user, (LIMIT + 1) as i64],
            |r| r.get::<_, String>(0),
        )?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    if peers.len() > LIMIT {
        return Err(invalid());
    }
    let mut retire = Vec::new();
    for peer in peers {
        let record = get(tx, &linked.scope.team, &linked.scope.user, &peer)?.ok_or_else(invalid)?;
        let original = &record.draft;
        // A resumed dialog can have a new visit token. No editor or draft clear
        // authority comes from this comparison: only the original open custody
        // is retired after the same complete content was accepted by Slack.
        if original.scope != linked.scope
            || original.content.source != linked.content.source
            || original.content.title != linked.content.title
            || original.content.note != linked.content.note
        {
            continue;
        }
        if super::completed::confirms(&record, &destination.id) {
            retire.push(record);
        }
    }
    // Decode the complete bounded inventory before any deletion. A malformed
    // row aborts the receipt, draft and retirement transaction together.
    if retire.len() > 1 {
        return Err(invalid());
    }
    for record in retire {
        // If this very old result was already pruned by terminal retention,
        // keep its original open custody instead of inventing an orphan proof.
        if super::super::get(tx, &receipt.scope)?.as_ref() != Some(receipt) {
            continue;
        }
        super::completed::insert(tx, &record, linked, receipt)?;
        let count = tx.execute("DELETE FROM file_share_direct_opens WHERE team=?1 AND user=?2 AND peer=?3 AND attempt=?4 AND body=?5",
            params![record.draft.scope.team,record.draft.scope.user,record.peer,record.id,encode(&record)?])?;
        if count != 1 {
            return Err(invalid());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
