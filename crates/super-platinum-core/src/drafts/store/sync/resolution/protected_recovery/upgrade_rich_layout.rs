//! Exact schema transform and lossless inspection. Never execute untrusted DDL.
use super::*;
pub(super) const ANCHOR: &str = "composer_drafts_rich_anchor_v1";
const MARKER: &str = "composer_draft_rich_format";
const MAX_PRIOR: usize = 1024 * 1024;
const RICH_DDL: &str = "CREATE TABLE composer_draft_rich_format(singleton INTEGER PRIMARY KEY CHECK(singleton=1),version INTEGER NOT NULL CHECK(version=1),account TEXT NOT NULL,migration_id TEXT NOT NULL,prior TEXT NOT NULL);
 CREATE INDEX composer_drafts ON composer_draft_rich_format(singleton);";
const ADAPTER_DDL: &str = "DROP TABLE composer_draft_adapter_format;
 CREATE TABLE composer_draft_adapter_format(singleton INTEGER PRIMARY KEY CHECK(singleton=1),version INTEGER NOT NULL CHECK(version=2));
 INSERT INTO composer_draft_adapter_format VALUES(1,2);";
const CONTENT_DDL: &str =
    "CREATE TABLE composer_draft_rich_local(destination TEXT PRIMARY KEY,content TEXT NOT NULL);
 CREATE TABLE composer_draft_rich_writer(id TEXT PRIMARY KEY,content TEXT NOT NULL);";

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Receipt {
    pub(super) account: String,
    pub(super) migration_id: String,
    pub(super) prior: Image,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Stage {
    Renamed,
    Marker,
    Adapter,
    Content,
    BeforeCommit,
}
#[derive(Default)]
pub(super) struct Fault {
    #[cfg(test)]
    pub(super) at: Option<Stage>,
    #[cfg(test)]
    pub(super) deferred_commit: bool,
}
impl Fault {
    fn check(&self, _stage: Stage) -> Result<(), AppError> {
        #[cfg(test)]
        if self.at == Some(_stage) {
            return Err(invalid("synthetic rich migration fault"));
        }
        Ok(())
    }
}
pub(super) fn receipt(prior: Image, account: &str) -> Result<Receipt, AppError> {
    validate_prior(&prior, account)?;
    Ok(Receipt {
        account: account.to_owned(),
        migration_id: uuid::Uuid::new_v4().to_string(),
        prior,
    })
}
fn validate_prior(prior: &Image, account: &str) -> Result<(), AppError> {
    if serde_json::to_vec(prior)?.len() > MAX_PRIOR {
        return Err(invalid("rich migration evidence bound"));
    }
    if schema::classify(prior, account)? != Class::Protected7 {
        return Err(invalid("rich migration requires protected inventory"));
    }
    // This first unit only accepts quiescent durable state. Full runtime drain
    // authority is separate and cannot be inferred from these empty collections.
    let shadow = prior.shadow()?;
    let loaded = loading::load(&shadow, account, LoadMode::Current, &BTreeMap::new())
        .map_err(|_| invalid("rich protected inventory invalid"))?;
    for view in loaded.managed().values() {
        let state = view.row().state();
        if !matches!(state.workflow(), crate::drafts::sync::Workflow::Idle)
            || state.consumption().is_some()
            || state.blocked_observation().is_some()
        {
            return Err(invalid("rich migration unresolved managed operation"));
        }
    }
    if loaded.recovery().iter().any(|r| {
        matches!(
            r.phase(),
            RecoveryPhase::Pending | RecoveryPhase::SupersededUnknown
        )
    }) {
        return Err(invalid("rich migration unresolved recovery"));
    }
    for row in prior.table("composer_draft_continuity")?.rows.iter() {
        let team: Team = wire::strict(
            row.get(1)
                .ok_or_else(|| invalid("rich continuity shape"))?
                .text()?,
        )?;
        if team.pending.is_some() {
            return Err(invalid("rich migration pending team ticket"));
        }
    }
    if !prior.table("upload_receipts")?.rows.is_empty() {
        return Err(invalid("rich migration requires settled upload inventory"));
    }
    for table in [
        "composer_draft_slash_bindings",
        "composer_draft_slash_remote_jobs",
    ] {
        if prior.tables.get(table).is_some_and(|t| !t.rows.is_empty()) {
            return Err(invalid("rich migration active command binding"));
        }
    }
    // Any unresolved journal is a durable obligation, not merely a UI state.
    if prior.tables.contains_key("sp_slash_meta") {
        let records = crate::slash_commands::journal::protected_inventory(&shadow.0, account)
            .map_err(|_| invalid("rich slash inventory invalid"))?;
        if records.iter().any(|r| !r.retired()) {
            return Err(invalid("rich migration unretired command"));
        }
    }
    // The private format intentionally starts with no rich content. Follow-on
    // checkpoint support must version/adapt this exact audit before filling it.
    Ok(())
}
pub(super) fn install(conn: &Connection, receipt: &Receipt, fault: &Fault) -> Result<(), AppError> {
    conn.execute_batch("ALTER TABLE composer_drafts RENAME TO composer_drafts_rich_anchor_v1;")?;
    fault.check(Stage::Renamed)?;
    conn.execute_batch(RICH_DDL)?;
    conn.execute(
        "INSERT INTO composer_draft_rich_format VALUES(1,1,?1,?2,?3)",
        rusqlite::params![
            receipt.account,
            receipt.migration_id,
            serde_json::to_string(&receipt.prior)?
        ],
    )?;
    fault.check(Stage::Marker)?;
    conn.execute_batch(ADAPTER_DDL)?;
    fault.check(Stage::Adapter)?;
    conn.execute_batch(CONTENT_DDL)?;
    fault.check(Stage::Content)?;
    #[cfg(test)]
    if fault.deferred_commit {
        conn.execute_batch("CREATE TEMP TABLE rich_parent(id INTEGER PRIMARY KEY);
          CREATE TEMP TABLE rich_child(id INTEGER REFERENCES rich_parent(id) DEFERRABLE INITIALLY DEFERRED);
          INSERT INTO rich_child VALUES(1);")?;
    }
    fault.check(Stage::BeforeCommit)
}
pub(super) fn audit(image: &Image, account: &str) -> Result<Receipt, AppError> {
    image.shape()?;
    let table = image.table(MARKER)?;
    if table.columns != ["singleton", "version", "account", "migration_id", "prior"]
        || table.rows.len() != 1
    {
        return Err(invalid("rich marker shape"));
    }
    let row = &table.rows[0];
    if row.len() != 5
        || row[0] != snapshot::Cell::Integer(1)
        || row[1] != snapshot::Cell::Integer(1)
        || row[2].text()? != account
        || !valid_uuid(row[3].text()?)
        || row[4].text()?.len() > MAX_PRIOR
    {
        return Err(invalid("rich marker owner/version/bounds"));
    }
    let prior: Image = wire::strict(row[4].text()?)?;
    validate_prior(&prior, account)?;
    // The prior schema was matched against trusted fixed DDL by classify before
    // any shadow execution. Apply our own fixed SQL, never persisted new SQL.
    let receipt = Receipt {
        account: account.into(),
        migration_id: row[3].text()?.into(),
        prior,
    };
    let expected = receipt.prior.shadow()?;
    expected.0.execute_batch("PRAGMA legacy_alter_table=OFF;")?;
    install(&expected.0, &receipt, &Fault::default())?;
    if Image::read(&expected.0)? != *image {
        return Err(invalid("rich barrier schema or retained bytes changed"));
    }
    Ok(receipt)
}
