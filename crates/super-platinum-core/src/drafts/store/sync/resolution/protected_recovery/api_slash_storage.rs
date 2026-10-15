//! Immutable protected-to-slash binding. No unlock/clear/completion path yet.
use super::*;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Binding {
    pub(super) version: u32,
    pub(super) scope: Scope,
    pub(super) intent: String,
    pub(super) enrollment: String,
    pub(super) inventory: views::InventoryRecord,
    pub(super) writer: String,
    pub(super) serial: u64,
    pub(super) writer_receipt: String,
    pub(super) stream: Team,
    pub(super) before_revision: u64,
    pub(super) after_revision: u64,
    pub(super) after_row: String,
    pub(super) dispatch: SlashRecord,
}
fn count(conn: &Connection) -> Result<i64, AppError> {
    Ok(conn.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('composer_draft_slash_format','composer_draft_slash_bindings')",[], |r|r.get(0))?)
}
pub(super) fn require(conn: &Connection, account: &str) -> Result<(), AppError> {
    if count(conn)? != 2 {
        return Err(invalid("protected slash sidecar missing or partial"));
    }
    let rows = {
        let mut q =
            conn.prepare("SELECT singleton,version,account FROM composer_draft_slash_format")?;
        q.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?
    };
    if rows.len() != 1 || rows[0].0 != 1 || !matches!(rows[0].1, 1 | 2 | 3) || rows[0].2 != account
    {
        return Err(invalid("protected slash format or owner changed"));
    }
    history::format(conn, rows[0].1)?;
    adapter_schema::require(conn)?;
    owner(conn, account)?;
    slash_journal::protected_inventory(conn, account)
        .map_err(|_| invalid("slash journal unavailable"))?;
    Ok(())
}
pub(super) fn install(conn: &Connection, account: &str) -> Result<(), AppError> {
    match count(conn)? {
        0 => {
            slash_journal::protected_inventory(conn, account)
                .map_err(|_| invalid("slash journal required"))?;
            conn.execute_batch("CREATE TABLE composer_draft_slash_format(singleton INTEGER PRIMARY KEY CHECK(singleton=1),version INTEGER NOT NULL,account TEXT NOT NULL);
                CREATE TABLE composer_draft_slash_bindings(intent TEXT PRIMARY KEY,destination TEXT NOT NULL UNIQUE,enrollment TEXT NOT NULL,content TEXT NOT NULL);")?;
            conn.execute(
                "INSERT INTO composer_draft_slash_format VALUES(1,1,?1)",
                [account],
            )?;
        }
        2 => (),
        _ => return Err(invalid("partial protected slash schema")),
    }
    require(conn, account)?;
    all(conn, account)?;
    Ok(())
}
fn validate(binding: &Binding) -> Result<(), AppError> {
    let before = views::managed(binding.inventory.restore()?)
        .map_err(|_| invalid("missing protected binding row"))?;
    let after = wire::decode(
        &binding.scope,
        sql_number(binding.after_revision)?,
        7,
        &binding.after_row,
    )?;
    let next = successor(&before.row().inner, before.row().state().clone())?;
    if binding.version != 1
        || before.scope() != &binding.scope
        || before.row().lineage().enrollment() != binding.enrollment
        || !valid_uuid(&binding.intent)
        || !valid_uuid(&binding.writer)
        || binding.serial == 0
        || binding.before_revision != before.row().revision()
        || !after.matches(&next)
        || binding.after_revision != next.stored.revision
        || binding.stream.mode != Mode::Open
        || binding.stream.pending.is_some()
        || binding
            .stream
            .lease_id
            .as_deref()
            .is_none_or(|id| !valid_uuid(id))
        || binding.stream.epoch != before.row().lineage().verified_epoch()
        || !writers::slash_receipt_matches(
            &binding.writer_receipt,
            &binding.writer,
            binding.serial,
            &before,
        )?
        || binding.dispatch.id() != binding.intent
        || binding.dispatch.owner().account != binding.scope.account_id
        || binding.dispatch.owner().team != binding.scope.key.team
        || binding.dispatch.owner().channel != binding.scope.key.channel
        || binding.dispatch.owner().thread != binding.scope.key.thread
        || binding.dispatch.original() != &original(&before)
        || binding.dispatch.protected_input_revision() != binding.before_revision
        || !matches!(
            binding.dispatch.phase(),
            crate::slash_commands::controller::Phase::DispatchPossible
        )
    {
        return Err(invalid("protected slash binding inconsistent"));
    }
    crate::slash_commands::controller::Controller::restore(binding.dispatch.clone())
        .map_err(|_| invalid("invalid retained command"))?;
    sql_number(binding.serial)?;
    Ok(())
}
pub(super) fn all(conn: &Connection, account: &str) -> Result<Vec<Binding>, AppError> {
    let receipts = slash_journal::protected_inventory(conn, account)
        .map_err(|_| invalid("invalid command inventory"))?;
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for (id, key, enrollment, raw) in history::binding_rows(conn)? {
        let binding: Binding = wire::strict(&raw)?;
        validate(&binding)?;
        if !seen.insert(id.clone())
            || binding.scope.account_id != account
            || binding.intent != id
            || binding.enrollment != enrollment
            || key != serde_json::to_string(&binding.scope.key)?
            || !receipts
                .iter()
                .any(|r| r.protected_descends_from(&binding.dispatch))
        {
            return Err(invalid("duplicate/changed historical binding"));
        }
        out.push(binding);
    }
    Ok(out)
}
pub(super) fn insert(conn: &Connection, binding: &Binding) -> Result<(), AppError> {
    require(conn, &binding.scope.account_id)?;
    validate(binding)?;
    if history::v2(conn)? {
        archive_storage::audit(conn, &binding.scope.account_id)?;
    }
    let raw = serde_json::to_string(binding)?;
    if raw.len() > 1024 * 1024 {
        return Err(invalid("protected slash binding bound"));
    }
    conn.execute(
        "INSERT INTO composer_draft_slash_bindings VALUES(?1,?2,?3,?4)",
        params![
            binding.intent,
            serde_json::to_string(&binding.scope.key)?,
            binding.enrollment,
            raw
        ],
    )?;
    all(conn, &binding.scope.account_id)?;
    Ok(())
}
/// Audited immutable binding plus its exact persisted bytes; no live authority.
pub(super) fn exact(
    conn: &Connection,
    scope: &Scope,
    id: &str,
) -> Result<(Binding, String), AppError> {
    require(conn, &scope.account_id)?;
    let binding = all(conn, &scope.account_id)?
        .into_iter()
        .find(|b| b.scope == *scope && b.intent == id)
        .ok_or_else(|| invalid("missing exact command binding"))?;
    let raw = history::binding_raw(conn, id)?;
    Ok((binding, raw))
}
pub(super) fn guard(conn: &Connection, scope: &Scope) -> Result<(), AppError> {
    if count(conn)? == 0 {
        let retained: i64 = conn.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('composer_draft_slash_archives','composer_draft_slash_attestations','composer_draft_slash_completion_format','composer_draft_slash_completions','composer_draft_slash_remote_jobs','composer_draft_slash_remote_archives')",[],|r|r.get(0))?;
        if retained != 0 {
            return Err(invalid("orphan protected slash retention schema"));
        }
        return Ok(());
    }
    require(conn, &scope.account_id)?;
    all(conn, &scope.account_id)?;
    if history::v2(conn)? {
        archive_storage::audit(conn, &scope.account_id)?;
    }
    let active: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM composer_draft_slash_bindings WHERE destination=?1)",
        [serde_json::to_string(&scope.key)?],
        |r| r.get(0),
    )?;
    if active {
        return Err(invalid(
            "protected slash intent requires explicit completion/recovery",
        ));
    }
    Ok(())
}
