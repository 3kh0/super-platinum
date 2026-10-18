//! Immutable attestations and archived ownership, bounded and fully audited.
use super::*;
use crate::slash_commands::controller::{ManagedEvidence, ManagedTerminal, Phase};
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Attestation {
    version: u32,
    scope: Scope,
    intent: String,
    binding: String,
    evidence: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ArchivedRecord {
    pub(super) version: u32,
    pub(super) scope: Scope,
    pub(super) intent: String,
    pub(super) binding: String,
    pub(super) settlement: String,
    pub(super) attestation: String,
    pub(super) reason: archive::Reason,
    #[serde(deserialize_with = "required_option")]
    pub(super) choice_id: Option<String>,
    pub(super) writer: String,
    pub(super) serial: u64,
    pub(super) writer_receipt: String,
    pub(super) team: Team,
    pub(super) before: views::InventoryRecord,
    pub(super) after: views::InventoryRecord,
    pub(super) retired: SlashRecord,
}
fn validate_attestation(conn: &Connection, raw: &str) -> Result<Attestation, AppError> {
    let a: Attestation = wire::strict(raw)?;
    let index = completion::validated(conn, &a.scope.account_id)?;
    validate_attestation_with(&index, a)
}
fn validate_attestation_with(
    index: &completion::ValidatedSettlements,
    a: Attestation,
) -> Result<Attestation, AppError> {
    let (binding, bytes) = index.bindings().exact(&a.scope, &a.intent)?;
    let (terminal, _) = index.settlement(&a.scope, &a.intent)?;
    let evidence = ManagedEvidence::decode(&a.evidence)
        .map_err(|_| invalid("invalid managed delivery evidence"))?;
    if a.version != 1
        || a.binding != bytes
        || a.intent != terminal.id()
        || !evidence.matches(&binding.dispatch, &terminal)
    {
        return Err(invalid("delivery evidence mismatch"));
    }
    Ok(a)
}
pub(super) fn proof(raw: &str) -> Result<ManagedEvidence, AppError> {
    let a: Attestation = wire::strict(raw)?;
    ManagedEvidence::decode(&a.evidence).map_err(|_| invalid("invalid retained outcome"))
}
pub(super) fn attestation_raw(
    conn: &Connection,
    account: &str,
    id: &str,
) -> Result<Option<String>, AppError> {
    history::require(conn, account)?;
    let raw:Option<String>=conn.query_row("SELECT CASE WHEN LENGTH(CAST(content AS BLOB))<=4194304 THEN content ELSE NULL END FROM composer_draft_slash_attestations WHERE intent=?1",[id],|r|r.get(0)).optional()?;
    if let Some(raw) = &raw {
        let a = validate_attestation(conn, raw)?;
        if a.scope.account_id != account || a.intent != id {
            return Err(invalid("attestation index changed"));
        }
    }
    Ok(raw)
}
/// Called only inside the same transaction as its terminal/settlement insertion.
pub(super) fn attest(
    conn: &Connection,
    scope: &Scope,
    outcome: &ManagedTerminal,
    retry: bool,
) -> Result<(), AppError> {
    history::require(conn, &scope.account_id)?;
    if conn.is_autocommit() {
        return Err(invalid("attestation requires completion transaction"));
    }
    let (binding, raw) = bindings::exact(conn, scope, outcome.terminal().id())?;
    if binding.dispatch != *outcome.dispatch() {
        return Err(invalid("wrong managed dispatch"));
    }
    let a = Attestation {
        version: 1,
        scope: scope.clone(),
        intent: binding.intent,
        binding: raw,
        evidence: outcome
            .encoded()
            .map_err(|_| invalid("outcome encoding failed"))?,
    };
    let bytes = serde_json::to_string(&a)?;
    validate_attestation(conn, &bytes)?;
    match attestation_raw(conn, &scope.account_id, &a.intent)? {
        Some(prior) if prior == bytes => return Ok(()),
        Some(_) => return Err(invalid("contradictory attestation retry")),
        None if retry => return Err(invalid("unattested receipt cannot be promoted")),
        None => (),
    }
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(invalid("attestation bound"));
    }
    conn.execute(
        "INSERT INTO composer_draft_slash_attestations VALUES(?1,?2)",
        params![a.intent, bytes],
    )?;
    audit(conn, &scope.account_id)
}
fn check_archive(
    index: &completion::ValidatedSettlements,
    attestations: &std::collections::BTreeMap<String, String>,
    a: &ArchivedRecord,
) -> Result<(), AppError> {
    let (binding, raw) = index.bindings().exact(&a.scope, &a.intent)?;
    let (terminal, settlement) = index.settlement(&a.scope, &a.intent)?;
    let before =
        views::managed(a.before.restore()?).map_err(|_| invalid("archive predecessor absent"))?;
    let after =
        views::managed(a.after.restore()?).map_err(|_| invalid("archive successor absent"))?;
    archive::local_only(&before)?;
    let mut expected = views::local_from_review(&before);
    let row = successor(&before.row().inner, before.row().state().clone())?;
    expected.review.stored = Some(row.stored.clone());
    expected.row = Some(Row::new(row));
    let expected = views::managed(expected).map_err(|_| invalid("invalid archive successor"))?;
    let expected_retired = terminal
        .protected_retirement()
        .map_err(|_| invalid("unretirable terminal"))?;
    if a.version != 1
        || a.binding != raw
        || a.settlement != settlement
        || a.scope != *before.scope()
        || a.scope != *after.scope()
        || binding.enrollment != before.row().lineage().enrollment()
        || !after.same(&expected)
        || a.retired != expected_retired
        || a.team.mode != Mode::Open
        || a.team.pending.is_some()
        || a.team.epoch != before.row().lineage().verified_epoch()
        || a.team.lease_id.as_deref().is_none_or(|v| !valid_uuid(v))
        || !writers::slash_receipt_matches(&a.writer_receipt, &a.writer, a.serial, &before)?
        || attestations.get(&a.intent).map(String::as_str) != Some(a.attestation.as_str())
    {
        return Err(invalid("archive ownership changed"));
    }
    let proof = proof(&a.attestation)?;
    let reason = match (&a.reason, terminal.phase()) {
        (archive::Reason::ProvenNotSent, Phase::NotSent) => {
            proof.before_execute() && a.choice_id.is_none()
        }
        (
            archive::Reason::LocalAcknowledged,
            Phase::Completed(crate::slash_commands::api::Reply::Acknowledged { .. }),
        ) => a.choice_id.is_none(),
        (
            archive::Reason::RejectedPreserveRetire,
            Phase::Completed(crate::slash_commands::api::Reply::Rejected { .. }),
        ) => a.choice_id.as_deref().is_some_and(valid_uuid),
        _ => false,
    };
    let current = index.bindings().journal(&a.intent);
    if !reason || current != Some(&a.retired) {
        return Err(invalid("archive terminal/reason contradiction"));
    }
    Ok(())
}
/// Fully validated immutable evidence for one read-only phase of the existing
/// immediate transaction/savepoint. This is not a cache or execution authority.
/// Drop before any write and before committing/releasing its transaction.
pub(super) struct ReadPhase<'tx> {
    _transaction: &'tx Connection,
    index: completion::ValidatedSettlements,
    attestations: std::collections::BTreeMap<String, String>,
}
impl ReadPhase<'_> {
    pub(super) fn binding(
        &self,
        scope: &Scope,
        id: &str,
    ) -> Result<(&bindings::Binding, &str), AppError> {
        self.index.bindings().exact(scope, id)
    }
    pub(super) fn settlement(
        &self,
        scope: &Scope,
        id: &str,
    ) -> Result<(&SlashRecord, &str), AppError> {
        self.index.settlement(scope, id)
    }
    pub(super) fn attestation(&self, id: &str) -> Option<&str> {
        self.attestations.get(id).map(String::as_str)
    }
}
pub(super) fn read_phase<'tx>(
    conn: &'tx Connection,
    account: &str,
) -> Result<ReadPhase<'tx>, AppError> {
    if conn.is_autocommit() {
        return Err(invalid(
            "archive read phase requires an existing transaction",
        ));
    }
    validated(conn, account)
}
pub(super) fn audit(conn: &Connection, account: &str) -> Result<(), AppError> {
    validated(conn, account).map(|_| ())
}
fn validated<'tx>(conn: &'tx Connection, account: &str) -> Result<ReadPhase<'tx>, AppError> {
    history::require(conn, account)?;
    let index = completion::validated(conn, account)?;
    let(n,size):(i64,i64)=conn.query_row("SELECT COUNT(*),COALESCE(SUM(LENGTH(CAST(content AS BLOB))+LENGTH(CAST(intent AS BLOB))),0) FROM (SELECT intent,content FROM composer_draft_slash_archives UNION ALL SELECT intent,content FROM composer_draft_slash_attestations UNION ALL SELECT intent,content FROM composer_draft_slash_bindings UNION ALL SELECT intent,binding AS content FROM composer_draft_slash_archives)",[],|r|Ok((r.get(0)?,r.get(1)?)))?;
    // Combined binding bytes plus history and attestation, never separate resettable budgets.
    if !(0..=12288).contains(&n) || !(0..=16777216).contains(&size) {
        return Err(invalid("slash archival evidence bound"));
    }
    let mut q=conn.prepare("SELECT intent,CASE WHEN LENGTH(CAST(content AS BLOB))<=4194304 THEN content ELSE NULL END FROM composer_draft_slash_attestations")?;
    let mut attestations = std::collections::BTreeMap::new();
    for row in q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (id, raw) = row?;
        let a = validate_attestation_with(&index, wire::strict(&raw)?)?;
        if a.intent != id || a.scope.account_id != account {
            return Err(invalid("attestation index mismatch"));
        }
        if attestations.insert(id, raw).is_some() {
            return Err(invalid("duplicate attestation"));
        }
    }
    let mut q=conn.prepare("SELECT intent,destination,enrollment,binding,CASE WHEN LENGTH(CAST(content AS BLOB))<=4194304 THEN content ELSE NULL END FROM composer_draft_slash_archives")?;
    for row in q.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
        ))
    })? {
        let (id, key, enrollment, binding, raw) = row?;
        let a: ArchivedRecord = wire::strict(&raw)?;
        if a.intent != id
            || a.scope.account_id != account
            || serde_json::to_string(&a.scope.key)? != key
            || a.binding != binding
        {
            return Err(invalid("archive indexes changed"));
        }
        let b: bindings::Binding = wire::strict(&binding)?;
        if b.enrollment != enrollment {
            return Err(invalid("archive enrollment changed"));
        }
        check_archive(&index, &attestations, &a)?;
    }
    if remote::enabled(conn)? {
        remote::audit_remote(conn, account)?;
    }
    Ok(ReadPhase {
        _transaction: conn,
        index,
        attestations,
    })
}
pub(super) fn move_active(conn: &Connection, a: &ArchivedRecord) -> Result<(), AppError> {
    history::require(conn, &a.scope.account_id)?;
    if conn.is_autocommit() {
        return Err(invalid("archive requires one transaction"));
    }
    let b: bindings::Binding = wire::strict(&a.binding)?;
    let bytes = serde_json::to_string(a)?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(invalid("archive entry bound"));
    }
    conn.execute(
        "INSERT INTO composer_draft_slash_archives VALUES(?1,?2,?3,?4,?5)",
        params![
            a.intent,
            serde_json::to_string(&a.scope.key)?,
            b.enrollment,
            a.binding,
            bytes
        ],
    )?;
    let n=conn.execute("DELETE FROM composer_draft_slash_bindings WHERE intent=?1 AND destination=?2 AND enrollment=?3 AND content=?4",params![a.intent,serde_json::to_string(&a.scope.key)?,b.enrollment,a.binding])?;
    if n != 1 {
        return Err(invalid("active command changed"));
    }
    audit(conn, &a.scope.account_id)
}
