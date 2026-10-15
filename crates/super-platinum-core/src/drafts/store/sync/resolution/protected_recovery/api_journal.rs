//! Exact completed-call receipts. These are read-only retry evidence, never
//! request capabilities and never a substitute for current projection authority.
use super::*;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Input {
    Mutation(facade::Outcome),
    Check(CheckResult),
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Entry {
    pub(super) version: u32,
    pub(super) id: String,
    pub(super) scope: Scope,
    pub(super) enrollment: String,
    pub(super) operation: Operation,
    pub(super) input: Input,
    pub(super) positive_accepted: bool,
    #[serde(deserialize_with = "required_option")]
    pub(super) original: Option<views::InventoryRecord>,
}
pub(super) fn inventory(conn: &Connection) -> Result<BTreeMap<String, Entry>, AppError> {
    adapter_schema::require(conn)?;
    super::super::inventory::bound(
        conn,
        "SELECT COUNT(*),COALESCE(SUM(LENGTH(CAST(id AS BLOB))+LENGTH(CAST(team AS BLOB))+LENGTH(CAST(content AS BLOB))),0) FROM composer_draft_execution_result",
    )?;
    let mut q = conn.prepare("SELECT id,team,content FROM composer_draft_execution_result")?;
    let mut out = BTreeMap::new();
    for raw in q.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })? {
        let (id, team, raw) = raw?;
        let shape: serde_json::Value = wire::strict(&raw)?;
        if shape.get("original").is_none() {
            return Err(invalid("missing completed-call projection guard"));
        }
        let record: Entry = wire::strict(&raw)?;
        let (prefix, nonce) = id
            .split_once(':')
            .ok_or_else(|| invalid("invalid completed-call key"))?;
        if record.version != 1
            || record.id != id
            || !valid_uuid(nonce)
            || !valid_uuid(&record.enrollment)
            || record.scope != record.operation.scope
            || record.scope.key.team != team
            || !valid_uuid(&record.scope.account_id)
            || !valid_uuid(&record.operation.id)
            || !valid_uuid(&record.operation.client_msg_id)
            || match &record.input {
                Input::Mutation(_) => prefix != "mutation" || nonce != record.operation.id,
                Input::Check(_) => prefix != "check",
            }
        {
            return Err(invalid("invalid completed-call identity"));
        }
        if let Some(original) = &record.original {
            let view = original.restore()?;
            if view.scope() != &record.scope
                || view
                    .row()
                    .is_none_or(|r| r.lineage().enrollment() != record.enrollment)
            {
                return Err(invalid("completed-call owner changed"));
            }
        }
        if record.positive_accepted
            && (!matches!(
                &record.input,
                Input::Mutation(
                    facade::Outcome::Active(_)
                        | facade::Outcome::Deleted { .. }
                        | facade::Outcome::Sent { .. }
                        | facade::Outcome::Scheduled { .. }
                ) | Input::Check(CheckResult::Positive(
                    Outcome::Active(_)
                        | Outcome::Deleted { .. }
                        | Outcome::Sent { .. }
                        | Outcome::Scheduled { .. }
                ))
            ) || record.original.is_none())
        {
            return Err(invalid("accepted positive lacks matching receipt"));
        }
        if out.insert(id, record).is_some() {
            return Err(invalid("duplicate completed-call identity"));
        }
    }
    Ok(out)
}
pub(super) fn save(conn: &Connection, entry: &Entry) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO composer_draft_execution_result VALUES(?1,?2,?3)",
        params![
            entry.id,
            entry.scope.key.team,
            serde_json::to_string(entry)?
        ],
    )?;
    inventory(conn)?;
    Ok(())
}
pub(super) fn matches(entry: &Entry, receipt: &Receipt, input: &Input) -> bool {
    entry.scope == *receipt.scope()
        && entry.enrollment == receipt.enrollment()
        && entry.operation == *receipt.operation()
        && entry.input == *input
}
pub(super) fn review(entry: &Entry) -> Result<Option<Review>, Error> {
    entry
        .original
        .as_ref()
        .map(|r| views::managed(r.restore().map_err(|_| Error::Storage)?))
        .transpose()
}
pub(super) fn positive(outcome: &Outcome) -> bool {
    !matches!(outcome, Outcome::Rejected | Outcome::Uncertain)
}
