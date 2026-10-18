//! Bounded durable custody. Retention is not authorization to replay or clear.
use super::*;
const MAX_RECORDS: i64 = 16;
const MAX_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Returned {
    Response(serde_json::Value),
    Unknown,
    RateLimited { retry_after_secs: u64 },
}
impl Returned {
    fn validate(&self) -> Result<(), AppError> {
        if let Self::Response(raw) = self {
            if !raw.is_object() || !model::bounded(raw, 1024 * 1024) {
                return Err(invalid("hosted response bound"));
            }
        }
        if matches!(
            self,
            Self::RateLimited {
                retry_after_secs: 0
            }
        ) {
            return Err(invalid("hosted retry interval missing"));
        }
        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    version: u32,
    id: String,
    scope: Scope,
    enrollment: String,
    inventory: views::InventoryRecord,
    writer: String,
    serial: u64,
    writer_receipt: String,
    stream: Team,
    before_revision: u64,
    after_revision: u64,
    after_row: String,
    input: Input,
    #[serde(deserialize_with = "required_option")]
    returned: Option<Returned>,
}
impl Record {
    pub fn scope(&self) -> &Scope {
        &self.scope
    }
    pub(super) fn same_original(&self, other: &Self) -> Result<bool, AppError> {
        let mut a = serde_json::to_value(self)?;
        let mut b = serde_json::to_value(other)?;
        a.as_object_mut().unwrap().remove("id");
        b.as_object_mut().unwrap().remove("id");
        Ok(a == b)
    }
    /// The saved draft predates the returned response. Compare every original
    /// input/custody field while keeping ordinary pre-send equality strict.
    pub(super) fn matches_saved_draft(&self, draft: &Self) -> Result<bool, AppError> {
        let mut original = self.clone();
        original.returned = None;
        draft.same_original(&original)
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn input(&self) -> &Input {
        &self.input
    }
    pub fn returned(&self) -> Option<&Returned> {
        self.returned.as_ref()
    }
    pub(super) fn new(
        before: &Review,
        writer: &Writer,
        serial: u64,
        receipt: &writers::Record,
        stream: Team,
        after: &ProtectedRow,
        input: Input,
    ) -> Result<Self, Error> {
        let value = Self {
            version: 1,
            id: uuid::Uuid::new_v4().to_string(),
            scope: before.scope().clone(),
            enrollment: writer.enrollment().to_owned(),
            inventory: disk(views::InventoryRecord::capture(&views::local_from_review(
                before,
            )))?,
            writer: writer.id().to_owned(),
            serial,
            writer_receipt: disk(serde_json::to_string(receipt))?,
            stream,
            before_revision: before.row().revision(),
            after_revision: after.stored.revision,
            after_row: disk(wire::encode(after))?,
            input,
            returned: None,
        };
        disk(value.validate())?;
        Ok(value)
    }
    pub(super) fn validate(&self) -> Result<(), AppError> {
        let before = views::managed(self.inventory.restore()?)
            .map_err(|_| invalid("hosted original row missing"))?;
        let after = wire::decode(
            &self.scope,
            sql_number(self.after_revision)?,
            7,
            &self.after_row,
        )?;
        let next = successor(&before.row().inner, before.row().state().clone())?;
        if self.version != 1
            || !valid_uuid(&self.id)
            || !valid_uuid(&self.writer)
            || self.serial == 0
            || before.scope() != &self.scope
            || before.row().lineage().enrollment() != self.enrollment
            || self.before_revision != before.row().revision()
            || self.after_revision != next.stored.revision
            || !after.matches(&next)
            || self.stream.mode != Mode::Open
            || self.stream.pending.is_some()
            || self
                .stream
                .lease_id
                .as_deref()
                .is_none_or(|id| !valid_uuid(id))
            || self.stream.epoch != before.row().lineage().verified_epoch()
            || !writers::slash_receipt_matches(
                &self.writer_receipt,
                &self.writer,
                self.serial,
                &before,
            )?
        {
            return Err(invalid("hosted original custody inconsistent"));
        }
        sql_number(self.serial)?;
        self.input
            .paired(&before)
            .map_err(|_| invalid("hosted original input changed"))?;
        if let Some(value) = &self.returned {
            value.validate()?;
        }
        Ok(())
    }
}
fn count(conn: &Connection) -> Result<i64, AppError> {
    Ok(conn.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('composer_draft_hosted_format','composer_draft_hosted_records')",[],|r|r.get(0))?)
}
pub(super) fn require(conn: &Connection, account: &str) -> Result<(), AppError> {
    if count(conn)? != 2 {
        return Err(invalid("hosted schema missing or partial"));
    }
    let mut q =
        conn.prepare("SELECT singleton,version,account FROM composer_draft_hosted_format LIMIT 2")?;
    let rows = q
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if rows != vec![(1, 1, account.to_owned())] {
        return Err(invalid("hosted format or owner changed"));
    }
    adapter_schema::require(conn)?;
    owner(conn, account)?;
    Ok(())
}
pub(super) fn install(conn: &Connection, account: &str) -> Result<(), AppError> {
    match count(conn)? {
        0 => {
            conn.execute_batch("CREATE TABLE composer_draft_hosted_format(singleton INTEGER PRIMARY KEY CHECK(singleton=1),version INTEGER NOT NULL,account TEXT NOT NULL);
                CREATE TABLE composer_draft_hosted_records(id TEXT PRIMARY KEY,destination TEXT NOT NULL UNIQUE,enrollment TEXT NOT NULL,content TEXT NOT NULL);")?;
            conn.execute(
                "INSERT INTO composer_draft_hosted_format VALUES(1,1,?1)",
                [account],
            )?;
        }
        2 => (),
        _ => return Err(invalid("partial hosted schema")),
    }
    all(conn, account)?;
    Ok(())
}
fn rows(conn: &Connection, account: &str) -> Result<Vec<(Record, String)>, AppError> {
    require(conn, account)?;
    // Aggregate byte limits are checked before materializing any journal content.
    let (count,total,largest):(i64,i64,i64)=conn.query_row("SELECT COUNT(*),COALESCE(SUM(length(CAST(content AS BLOB))+length(CAST(id AS BLOB))+length(CAST(destination AS BLOB))+length(CAST(enrollment AS BLOB))),0),COALESCE(MAX(length(CAST(content AS BLOB))),0) FROM composer_draft_hosted_records",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
    if count > MAX_RECORDS || total > MAX_RECORDS * MAX_BYTES as i64 || largest > MAX_BYTES as i64 {
        return Err(invalid("hosted inventory bound"));
    }
    let mut q = conn.prepare(
        "SELECT id,destination,enrollment,content FROM composer_draft_hosted_records ORDER BY id",
    )?;
    let mut out = Vec::new();
    let mut ids = std::collections::BTreeSet::new();
    let mut scopes = std::collections::BTreeSet::new();
    for row in q.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
        ))
    })? {
        let (id, key, enrollment, raw) = row?;
        let record: Record = wire::strict(&raw)?;
        record.validate()?;
        if record.scope.account_id != account
            || record.id != id
            || record.enrollment != enrollment
            || serde_json::to_string(&record.scope.key)? != key
            || !ids.insert(id)
            || !scopes.insert(key)
        {
            return Err(invalid("hosted indexed custody changed"));
        }
        out.push((record, raw));
    }
    Ok(out)
}
pub(super) fn all(conn: &Connection, account: &str) -> Result<Vec<Record>, AppError> {
    Ok(rows(conn, account)?.into_iter().map(|(r, _)| r).collect())
}
pub(super) fn insert(conn: &Connection, record: &Record) -> Result<(), AppError> {
    if all(conn, &record.scope.account_id)?.len() >= MAX_RECORDS as usize {
        return Err(invalid("hosted inventory full"));
    }
    record.validate()?;
    if record.returned.is_some() {
        return Err(invalid("hosted prepared outcome already present"));
    }
    let raw = serde_json::to_string(record)?;
    if raw.len() > MAX_BYTES {
        return Err(invalid("hosted record bound"));
    }
    conn.execute(
        "INSERT INTO composer_draft_hosted_records VALUES(?1,?2,?3,?4)",
        params![
            record.id,
            serde_json::to_string(&record.scope.key)?,
            record.enrollment,
            raw
        ],
    )?;
    all(conn, &record.scope.account_id)?;
    Ok(())
}
pub(super) fn settle(
    conn: &Connection,
    prepared: &Record,
    result: Returned,
) -> Result<Record, AppError> {
    prepared.validate()?;
    result.validate()?;
    if prepared.returned.is_some() {
        return Err(invalid("hosted original claim required"));
    }
    if let Some(done) = cleanup::historical_record(conn, &prepared.scope.account_id, prepared.id())?
    {
        if done.same_claim(prepared)? && done.returned() == Some(&result) {
            return Ok(done);
        }
        return Err(invalid("hosted historical result changed"));
    }
    let (mut current, raw) = rows(conn, &prepared.scope.account_id)?
        .into_iter()
        .find(|(r, _)| r.id == prepared.id)
        .ok_or_else(|| invalid("hosted exact claim missing"))?;
    let returned = current.returned.take();
    if serde_json::to_value(&current)? != serde_json::to_value(prepared)? {
        return Err(invalid("hosted claim changed"));
    }
    if let Some(prior) = returned {
        if prior != result {
            return Err(invalid("hosted returned outcome changed"));
        }
        current.returned = Some(prior);
        return Ok(current);
    }
    current.returned = Some(result);
    current.validate()?;
    let next = serde_json::to_string(&current)?;
    if next.len() > MAX_BYTES {
        return Err(invalid("hosted settled record bound"));
    }
    let changed = conn.execute(
        "UPDATE composer_draft_hosted_records SET content=?1 WHERE id=?2 AND content=?3",
        params![next, current.id, raw],
    )?;
    if changed != 1 {
        return Err(invalid("hosted settlement lost custody"));
    }
    all(conn, &prepared.scope.account_id)?;
    Ok(current)
}
pub(super) fn guard(conn: &Connection, scope: &Scope) -> Result<(), AppError> {
    if count(conn)? == 0 {
        return Ok(());
    }
    if all(conn, &scope.account_id)?
        .iter()
        .any(|r| r.scope == *scope)
    {
        return Err(invalid(
            "hosted files require explicit completion or recovery",
        ));
    }
    Ok(())
}

impl Record {
    pub(super) fn same_claim(&self, original: &Self) -> Result<bool, AppError> {
        let mut current = self.clone();
        current.returned = None;
        Ok(serde_json::to_value(current)? == serde_json::to_value(original)?)
    }
    pub(super) fn claimed(&self) -> Result<Review, AppError> {
        let mut inventory = self.inventory.restore()?;
        let row = wire::decode(
            &self.scope,
            sql_number(self.after_revision)?,
            7,
            &self.after_row,
        )?;
        inventory.review.stored = Some(row.stored.clone());
        inventory.row = Some(Row::new(row));
        views::managed(inventory).map_err(|_| invalid("hosted claimed inventory"))
    }
    pub(super) fn writer_matches(&self, conn: &Connection) -> Result<bool, AppError> {
        let current: Option<String> = conn
            .query_row(
                "SELECT content FROM composer_draft_writer_lineage WHERE writer_id=?1",
                [&self.writer],
                |r| r.get(0),
            )
            .optional()?;
        Ok(current.as_deref() == Some(self.writer_receipt.as_str()))
    }
    pub(super) fn stream_matches(
        &self,
        conn: &Connection,
        stream: &Stream,
    ) -> Result<bool, AppError> {
        if stream.account() != self.scope.account_id || stream.team() != self.scope.key.team {
            return Ok(false);
        }
        Ok(storage::team(conn, stream.team())? == self.stream)
    }
}
