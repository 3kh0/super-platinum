//! Association audit inside every admission transaction. JSON bodies stay in SQLite;
//! only bounded metadata reaches Rust, not every archived Markdown string.
use super::*;
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftLink {
    id: String,
    owner: Owner,
    revision: u64,
    writer: String,
    attempt: Option<String>,
    archived: bool,
    decision_revision: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AttemptLink {
    id: String,
    owner: Owner,
    draft_id: String,
    draft_revision: u64,
    delivery: Delivery,
    result_revision: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConflictLink {
    id: String,
    owner: Owner,
    draft_id: String,
    base_revision: u64,
    writer: String,
}

fn metadata<T: DeserializeOwned>(
    conn: &Connection,
    table: &str,
    remove: &str,
) -> Result<Vec<(String, T)>> {
    // table/remove are fixed internal literals. json_remove preserves all OTHER keys,
    // so unexpected metadata is rejected by deny_unknown_fields.
    let expr = format!("json_remove(body,'{remove}')");
    let mut stmt=conn.prepare(&format!("SELECT id,CASE WHEN length(CAST({expr} AS BLOB))<=65536 THEN {expr} ELSE NULL END FROM {table}"))?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    rows.map(|row| {
        let (id, body) = row?;
        let data = serde_json::from_str(&body).map_err(|_| Fault::Invalid)?;
        Ok((id, data))
    })
    .collect()
}
fn content_shape(conn: &Connection, table: &str, key: &str) -> Result<()> {
    let path = format!("$.{key}");
    let invalid: i64 = conn.query_row(
        &format!(
            "SELECT count(*) FROM {table} WHERE
      json_type(body,'{path}') IS NOT 'object'
      OR json_type(body,'{path}.title') IS NOT 'text'
      OR json_type(body,'{path}.markdown') IS NOT 'text'
      OR length(CAST(json_extract(body,'{path}.title') AS BLOB))>?1
      OR length(CAST(json_extract(body,'{path}.markdown') AS BLOB))>?2
      OR (SELECT count(*) FROM json_each(body,'{path}'))!=2"
        ),
        params![MAX_TITLE_BYTES as i64, MAX_MARKDOWN_BYTES as i64],
        |r| r.get(0),
    )?;
    if invalid != 0 {
        Err(Fault::Invalid)
    } else {
        Ok(())
    }
}
fn equal_content(conn: &Connection, draft: &str, attempt: &str) -> Result<bool> {
    let equal: bool = conn.query_row(
        "SELECT json_extract(d.body,'$.content')=json_extract(a.body,'$.frozen')
        FROM drafts d,attempts a WHERE d.id=?1 AND a.id=?2",
        params![draft, attempt],
        |r| r.get(0),
    )?;
    Ok(equal)
}
pub(super) fn validate(conn: &Connection, account: &str) -> Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let identity: (i64, String) =
        conn.query_row("SELECT count(*),account FROM identity", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
    if version != 2 || identity != (1, account.to_owned()) {
        return Err(Fault::Invalid);
    }
    for (table, max) in [
        ("scopes", MAX_SCOPES),
        ("drafts", MAX_SCOPES + MAX_ATTEMPTS),
        ("attempts", MAX_ATTEMPTS),
        ("conflicts", MAX_CONFLICTS),
        ("decisions", MAX_ATTEMPTS),
        ("dispositions", MAX_CONFLICT_OPERATIONS),
    ] {
        if count(conn, table)? > max {
            return Err(Fault::Invalid);
        }
        let invalid:i64=conn.query_row(&format!("SELECT count(*) FROM {table} WHERE length(CAST(body AS BLOB))>?1 OR NOT json_valid(body)"),[MAX_ROW_BYTES as i64],|r|r.get(0))?;
        if invalid != 0 {
            return Err(Fault::Invalid);
        }
    }
    if count(conn, "conflicts")?
        .checked_add(count(conn, "dispositions")?)
        .is_none_or(|n| n > MAX_CONFLICT_OPERATIONS)
    {
        return Err(Fault::Invalid);
    }
    content_shape(conn, "drafts", "content")?;
    content_shape(conn, "attempts", "frozen")?;
    content_shape(conn, "conflicts", "content")?;
    let scopes = all::<Scope>(conn, account)?
        .into_iter()
        .map(|s| (s.owner.key(), s))
        .collect::<BTreeMap<_, _>>();
    let decisions = all::<ArchiveReceipt>(conn, account)?;
    let dispositions = all::<Disposition>(conn, account)?;
    let mut drafts = BTreeMap::new();
    for (key, d) in metadata::<DraftLink>(conn, "drafts", "$.content")? {
        Draft {
            id: d.id.clone(),
            owner: d.owner.clone(),
            revision: d.revision,
            writer: d.writer.clone(),
            content: Content::empty(),
            attempt: d.attempt.clone(),
            archived: d.archived,
            decision_revision: d.decision_revision,
        }
        .validate()?;
        if key != d.id || d.owner.account != account {
            return Err(Fault::Invalid);
        }
        let s = scopes.get(&d.owner.key()).ok_or(Fault::Invalid)?;
        if (s.active.as_deref() == Some(d.id.as_str())) == d.archived {
            return Err(Fault::Invalid);
        }
        drafts.insert(key, d);
    }
    for s in scopes.values() {
        if let Some(id) = &s.active {
            let d = drafts.get(id).ok_or(Fault::Invalid)?;
            if d.owner != s.owner || d.archived {
                return Err(Fault::Invalid);
            }
        }
    }
    let mut attempts = BTreeMap::new();
    for (key, a) in metadata::<AttemptLink>(conn, "attempts", "$.frozen")? {
        Attempt {
            id: a.id.clone(),
            owner: a.owner.clone(),
            draft_id: a.draft_id.clone(),
            draft_revision: a.draft_revision,
            frozen: Content::new("metadata validation".into(), String::new())?,
            delivery: a.delivery.clone(),
            result_revision: a.result_revision,
        }
        .validate()?;
        if key != a.id || a.owner.account != account {
            return Err(Fault::Invalid);
        }
        let d = drafts.get(&a.draft_id).ok_or(Fault::Invalid)?;
        let scope = scopes.get(&a.owner.key()).ok_or(Fault::Invalid)?;
        if d.owner != a.owner
            || d.revision < a.draft_revision
            || (d.revision == a.draft_revision && !equal_content(conn, &d.id, &a.id)?)
        {
            return Err(Fault::Invalid);
        }
        if let Delivery::RateLimited { retry_at } = a.delivery {
            if scope.retry_at < retry_at {
                return Err(Fault::Invalid);
            }
        }
        if matches!(
            a.delivery,
            Delivery::Reserved | Delivery::Started | Delivery::Uncertain | Delivery::Created { .. }
        ) {
            if d.attempt.as_deref() != Some(a.id.as_str()) || d.revision != a.draft_revision {
                return Err(Fault::Invalid);
            }
            if d.archived {
                if matches!(a.delivery, Delivery::Reserved) {
                    return Err(Fault::Invalid);
                }
                let matches = decisions
                    .iter()
                    .filter(|decision| decision.old_draft == d.id && decision.old_attempt == a.id)
                    .count();
                if matches != 1 {
                    return Err(Fault::Invalid);
                }
            } else if scope.active.as_deref() != Some(d.id.as_str()) {
                return Err(Fault::Invalid);
            }
        }
        attempts.insert(key, a);
    }
    for d in drafts.values() {
        if let Some(id) = &d.attempt {
            let a = attempts.get(id).ok_or(Fault::Invalid)?;
            if a.owner != d.owner || a.draft_id != d.id || a.draft_revision != d.revision {
                return Err(Fault::Invalid);
            }
        }
    }
    let mut decided = BTreeMap::new();
    for decision in &decisions {
        let d = drafts.get(&decision.old_draft).ok_or(Fault::Invalid)?;
        let a = attempts.get(&decision.old_attempt).ok_or(Fault::Invalid)?;
        if d.owner != decision.owner
            || a.owner != decision.owner
            || !d.archived
            || d.revision != decision.expected_draft_revision
            || a.draft_id != d.id
            || d.attempt.as_deref() != Some(a.id.as_str())
            || d.decision_revision != next(decision.expected_decision_revision)?
            || a.result_revision < decision.expected_result_revision
            || decided
                .insert(d.id.clone(), decision.operation.clone())
                .is_some()
        {
            return Err(Fault::Invalid);
        }
        match decision.reason {
            ArchiveReason::AbandonedOutcomeUnknown => {
                if !matches!(
                    decision.expected_delivery,
                    Delivery::Started | Delivery::Uncertain
                ) || matches!(
                    a.delivery,
                    Delivery::Reserved | Delivery::CanceledBeforeAdmission
                ) {
                    return Err(Fault::Invalid);
                }
            }
            ArchiveReason::CreatedReviewed => {
                if !matches!(decision.expected_delivery, Delivery::Created { .. })
                    || a.delivery != decision.expected_delivery
                    || a.result_revision != decision.expected_result_revision
                {
                    return Err(Fault::Invalid);
                }
            }
        }
        // The new draft may have been explicitly discarded later. Never recreate it.
        if let Some(new) = drafts.get(&decision.new_draft) {
            if new.owner != decision.owner {
                return Err(Fault::Invalid);
            }
        }
    }
    let mut retired = BTreeMap::new();
    for disposition in dispositions {
        if !scopes.contains_key(&disposition.owner.key()) {
            return Err(Fault::Invalid);
        }
        retired.insert(disposition.id, ());
    }
    for (key, c) in metadata::<ConflictLink>(conn, "conflicts", "$.content")? {
        Disposition {
            id: c.id.clone(),
            owner: c.owner.clone(),
            draft_id: c.draft_id.clone(),
            base_revision: c.base_revision,
            writer: c.writer,
        }
        .validate()?;
        if key != c.id
            || c.owner.account != account
            || retired.contains_key(&c.id)
            || !scopes.contains_key(&c.owner.key())
        {
            return Err(Fault::Invalid);
        }
        if let Some(d) = drafts.get(&c.draft_id) {
            if d.owner != c.owner || c.base_revision > d.revision {
                return Err(Fault::Invalid);
            }
        }
    }
    Ok(())
}

pub(super) fn has_conflicts(conn: &Connection, owner: &Owner) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT count(*) FROM conflicts WHERE json_extract(body,'$.owner.account')=?1
      AND json_extract(body,'$.owner.team')=?2 AND json_extract(body,'$.owner.user')=?3",
        params![owner.account, owner.team, owner.user],
        |r| r.get(0),
    )?;
    Ok(count > 0)
}
