//! Immutable completion evidence. No release, replay, or capability restoration.
use super::*;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Receipt {
    pub(super) version: u32,
    pub(super) scope: Scope,
    pub(super) intent: String,
    pub(super) binding: String,
    pub(super) terminal: SlashRecord,
    pub(super) effect: Effect,
    #[serde(deserialize_with = "required_option")]
    pub(super) before: Option<views::InventoryRecord>,
    #[serde(deserialize_with = "required_option")]
    pub(super) after: Option<views::InventoryRecord>,
}
fn count(conn: &Connection) -> Result<i64, AppError> {
    Ok(conn.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('composer_draft_slash_completion_format','composer_draft_slash_completions')",[],|r|r.get(0))?)
}
pub(super) fn require(conn: &Connection, account: &str) -> Result<(), AppError> {
    bindings::require(conn, account)?;
    if count(conn)? != 2 {
        return Err(invalid("missing completion schema"));
    }
    let mut q = conn
        .prepare("SELECT singleton,version,account FROM composer_draft_slash_completion_format")?;
    let rows = q
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if rows != [(1, 1, account.to_owned())] {
        return Err(invalid("completion schema/owner changed"));
    }
    Ok(())
}
pub(super) fn install(conn: &Connection, account: &str) -> Result<(), AppError> {
    if conn.is_autocommit() {
        return Err(invalid("completion migration requires transaction"));
    }
    match count(conn)? {
        0 => {
            bindings::require(conn, account)?;
            conn.execute_batch("CREATE TABLE composer_draft_slash_completion_format(singleton INTEGER PRIMARY KEY CHECK(singleton=1),version INTEGER NOT NULL,account TEXT NOT NULL);
                CREATE TABLE composer_draft_slash_completions(intent TEXT PRIMARY KEY,destination TEXT NOT NULL,content TEXT NOT NULL);")?;
            conn.execute(
                "INSERT INTO composer_draft_slash_completion_format VALUES(1,1,?1)",
                [account],
            )?;
        }
        2 => (),
        _ => return Err(invalid("partial completion schema")),
    }
    all(conn, account)?;
    Ok(())
}
fn view(value: Option<&views::InventoryRecord>) -> Result<Option<Review>, AppError> {
    value
        .map(|v| {
            views::managed(v.restore()?).map_err(|_| invalid("completion inventory has no row"))
        })
        .transpose()
}
// The journal permits exactly one later transition of a settled terminal:
// explicit retirement preserving phase, HTTP evidence and every frozen byte.
fn same_terminal_or_retired(current: &SlashRecord, saved: &SlashRecord) -> Result<bool, AppError> {
    if current == saved {
        return Ok(true);
    }
    if saved.retired() || !current.retired() || pending(saved) {
        return Ok(false);
    }
    let mut expected = serde_json::to_value(saved)?;
    let version = expected["version"]
        .as_u64()
        .and_then(|n| n.checked_add(1))
        .ok_or_else(|| invalid("completion version overflow"))?;
    expected["version"] = serde_json::json!(version);
    expected["retired"] = serde_json::json!(true);
    Ok(expected == serde_json::to_value(current)?)
}
fn validate(conn: &Connection, receipt: &Receipt) -> Result<(), AppError> {
    let (binding, raw) = bindings::exact(conn, &receipt.scope, &receipt.intent)?;
    let terminal = &receipt.terminal;
    crate::slash_commands::controller::Controller::restore(terminal.clone())
        .map_err(|_| invalid("invalid terminal command"))?;
    if receipt.version != 1
        || receipt.binding != raw
        || terminal.id() != receipt.intent
        || !terminal.protected_descends_from(&binding.dispatch)
        || matches!(terminal.phase(), Phase::Prepared | Phase::DispatchPossible)
    {
        return Err(invalid("completion binding changed"));
    }
    let journal = slash_journal::protected_inventory(conn, &receipt.scope.account_id)
        .map_err(|_| invalid("command journal unavailable"))?;
    let current = journal
        .iter()
        .find(|r| r.id() == receipt.intent)
        .ok_or_else(|| invalid("missing terminal command"))?;
    if !same_terminal_or_retired(current, terminal)? {
        return Err(invalid("completion contradicts journal"));
    }
    let before = view(receipt.before.as_ref())?;
    let after = view(receipt.after.as_ref())?;
    if before
        .iter()
        .chain(after.iter())
        .any(|v| v.scope() != &receipt.scope)
    {
        return Err(invalid("completion inventory owner changed"));
    }
    let consistent = match receipt.effect {
        Effect::PendingUnknown => pending(terminal),
        Effect::RetainedNotSent => matches!(terminal.phase(), Phase::NotSent),
        Effect::RetainedRejected => matches!(
            terminal.phase(),
            Phase::Completed(SlashReply::Rejected { .. })
        ),
        Effect::RetainedKeepInput => matches!(
            terminal.phase(),
            Phase::Completed(SlashReply::Acknowledged {
                keep_input: true,
                ..
            })
        ),
        Effect::ClearedExactAcknowledged | Effect::PreservedCurrent => matches!(
            terminal.phase(),
            Phase::Completed(SlashReply::Acknowledged {
                keep_input: false,
                ..
            })
        ),
        Effect::RecoveredPreserved => !pending(terminal),
    };
    if !consistent {
        return Err(invalid("completion disposition contradicts response"));
    }
    if receipt.effect != Effect::ClearedExactAcknowledged {
        if !same_view(before.as_ref(), after.as_ref()) {
            return Err(invalid("retained completion changed input"));
        }
        return Ok(());
    }
    let before = before.ok_or_else(|| invalid("missing cleared predecessor"))?;
    let after = after.ok_or_else(|| invalid("missing cleared successor"))?;
    if !before.same(&after_claim(&binding).map_err(|_| invalid("invalid claim successor"))?)
        || before.local().empty()
    {
        return Err(invalid("clear predecessor changed"));
    }
    let state = before
        .row()
        .state()
        .clear_acknowledged_slash_input()
        .ok_or_else(|| invalid("clear state unavailable"))?;
    let row = successor(&before.row().inner, state)?;
    let mut expected = views::local_from_review(&before);
    expected.review.local = LocalDraft::default();
    expected.review.stored = Some(row.stored.clone());
    expected.row = Some(Row::new(row));
    // write_local touches once and DELETE of the known nonempty row touches once.
    expected.clock = expected
        .clock
        .checked_add(2)
        .ok_or_else(|| invalid("local clock overflow"))?;
    expected.generation = None;
    let expected = views::managed(expected).map_err(|_| invalid("invalid clear successor"))?;
    if !after.same(&expected) {
        return Err(invalid("clear changed remote ownership or successor"));
    }
    Ok(())
}
fn all(conn: &Connection, account: &str) -> Result<Vec<Receipt>, AppError> {
    require(conn, account)?;
    let (n,bytes):(i64,i64)=conn.query_row("SELECT COUNT(*),COALESCE(SUM(LENGTH(CAST(content AS BLOB))+LENGTH(CAST(intent AS BLOB))+LENGTH(CAST(destination AS BLOB))),0) FROM composer_draft_slash_completions",[],|r|Ok((r.get(0)?,r.get(1)?)))?;
    if !(0..=4096).contains(&n) || !(0..=16777216).contains(&bytes) {
        return Err(invalid("completion inventory bound"));
    }
    let mut q=conn.prepare("SELECT intent,destination,CASE WHEN LENGTH(CAST(content AS BLOB))<=4194304 THEN content ELSE NULL END FROM composer_draft_slash_completions")?;
    let mut result = Vec::new();
    for row in q.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })? {
        let (id, key, raw) = row?;
        let receipt: Receipt = wire::strict(&raw)?;
        if receipt.intent != id
            || receipt.scope.account_id != account
            || serde_json::to_string(&receipt.scope.key)? != key
        {
            return Err(invalid("completion index changed"));
        }
        validate(conn, &receipt)?;
        result.push(receipt);
    }
    Ok(result)
}
pub(super) fn audit(conn: &Connection, account: &str) -> Result<(), AppError> {
    all(conn, account).map(|_| ())
}
pub(super) fn read(
    conn: &Connection,
    account: &str,
    id: &str,
) -> Result<Option<Receipt>, AppError> {
    Ok(all(conn, account)?.into_iter().find(|r| r.intent == id))
}
pub(super) fn insert(conn: &Connection, receipt: &Receipt) -> Result<(), AppError> {
    require(conn, &receipt.scope.account_id)?;
    validate(conn, receipt)?;
    let raw = serde_json::to_string(receipt)?;
    if raw.len() > 4 * 1024 * 1024 {
        return Err(invalid("completion receipt bound"));
    }
    conn.execute(
        "INSERT INTO composer_draft_slash_completions VALUES(?1,?2,?3)",
        params![
            receipt.intent,
            serde_json::to_string(&receipt.scope.key)?,
            raw
        ],
    )?;
    all(conn, &receipt.scope.account_id)?;
    Ok(())
}
