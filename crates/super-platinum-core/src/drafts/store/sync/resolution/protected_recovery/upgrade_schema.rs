//! Recognize whole schema groups, then audit a shadow; never repair the source.
use super::*;
use snapshot::{Cell, Image};
const UPLOADS: &[&str] = &["upload_journal_identity", "upload_receipts"];
const HOSTED: &[&str] = &[
    "composer_draft_hosted_format",
    "composer_draft_hosted_records",
];
const SLASH: &[&str] = &["sp_slash_meta", "sp_slash_intents", "sp_slash_cooldowns"];
pub(super) const HISTORY_SQL: &str = "CREATE TABLE composer_draft_migration_receipt(singleton INTEGER PRIMARY KEY CHECK(singleton=1),version INTEGER NOT NULL CHECK(version=1),account TEXT NOT NULL,id TEXT NOT NULL,source TEXT NOT NULL,result TEXT NOT NULL)";

fn group(image: &Image, names: &[&str]) -> Result<bool, AppError> {
    let n = names
        .iter()
        .filter(|n| image.tables.contains_key(**n))
        .count();
    if n != 0 && n != names.len() {
        return Err(invalid("partial upgrade schema group"));
    }
    Ok(n != 0)
}
fn marker(image: &Image, name: &str) -> Result<i64, AppError> {
    let table = image.table(name)?;
    if table.rows.len() != 1 || table.rows[0].first() != Some(&Cell::Integer(1)) {
        return Err(invalid("malformed upgrade marker"));
    }
    table.rows[0]
        .get(1)
        .ok_or_else(|| invalid("missing version"))?
        .integer()
}
pub(super) fn ordinary() -> Result<DraftStore, AppError> {
    DraftStore::open("")
}
pub(super) fn classify(image: &Image, account: &str) -> Result<Class, AppError> {
    image.shape()?;
    if image.user_version != 0 || image.application_id != 0 {
        return Err(invalid("unknown SQLite application metadata"));
    }
    let uploads = group(image, UPLOADS)?;
    let slash = group(image, SLASH)?;
    let hosted = group(image, HOSTED)?;
    let hosted_editor = image.tables.contains_key("composer_draft_hosted_drafts");
    let hosted_cleanup = image
        .tables
        .contains_key("composer_draft_hosted_completions");
    if hosted_cleanup && !hosted_editor {
        return Err(invalid("orphan hosted completion schema"));
    }
    if hosted_editor && !hosted {
        return Err(invalid("orphan hosted editor schema"));
    }
    let protected = image
        .tables
        .contains_key("composer_draft_continuity_format");
    let binding = image.tables.contains_key("composer_draft_slash_format");
    let completed = group(
        image,
        &[
            "composer_draft_slash_completion_format",
            "composer_draft_slash_completions",
        ],
    )?;
    let history = image
        .tables
        .contains_key("composer_draft_migration_receipt");
    let local_only = image.tables.len() == 1 && image.tables.contains_key("composer_drafts");
    if (binding && !slash)
        || (completed && !binding)
        || (history && !protected)
        || (hosted && !protected)
    {
        return Err(invalid("orphan upgrade sidecar"));
    }
    let mut expected = ordinary()?;
    if !uploads {
        expected
            .0
            .execute_batch("DROP TABLE upload_receipts; DROP TABLE upload_journal_identity;")?;
    }
    if protected {
        if marker(image, "composer_draft_continuity_format")? != 7 {
            return Err(invalid("unsupported continuity format"));
        }
        storage::activate(&expected, account)?;
        adapter_schema::install(&expected, account)?;
    }
    if slash {
        let version = if binding {
            marker(image, "composer_draft_slash_format")?
        } else {
            0
        };
        if binding && (!protected || !matches!(version, 1..=3)) {
            return Err(invalid("unsupported slash format"));
        }
        slash_intent::upgrade_reference(&mut expected, account, version, completed)?;
    }
    if hosted {
        if marker(image, "composer_draft_hosted_format")? != 1 {
            return Err(invalid("unsupported hosted format"));
        }
        let tx = adapter_atomic::begin(&expected.0)?;
        hosted_files::upgrade_reference(&tx, account, hosted_editor, hosted_cleanup)?;
        tx.commit()?;
    }
    if history {
        expected.0.execute_batch(HISTORY_SQL)?;
    }
    let expected_image = Image::read(&expected.0)?;
    for (name, table) in &image.tables {
        if expected_image
            .tables
            .get(name)
            .is_none_or(|expected| expected.columns != table.columns)
        {
            return Err(invalid("persisted column schema changed"));
        }
    }
    let mut expected_schema = expected_image.schema;
    // SQLite retains sqlite_sequence after its final AUTOINCREMENT table drops.
    if !uploads {
        expected_schema.retain(|o| o.name != "sqlite_sequence");
    }
    if local_only {
        expected_schema.retain(|o| o.table == "composer_drafts");
    }
    // Exact older local-only DDL appears in the existing migration regression.
    // Accept this one known spelling; never normalize arbitrary SQL strings.
    for object in &image.schema {
        if object.name == "composer_drafts"
            && object.sql.as_deref()
                == Some(
                    "CREATE TABLE composer_drafts(destination TEXT PRIMARY KEY,content TEXT NOT NULL)",
                )
        {
            if let Some(expected) = expected_schema.iter_mut().find(|v| v.name == object.name) {
                expected.sql = object.sql.clone();
            }
        }
    }
    if image.schema != expected_schema {
        return Err(invalid("unknown or partial SQLite schema"));
    }
    let empty = Vec::new();
    let owners = image
        .tables
        .get("composer_draft_owner")
        .map_or(&empty, |t| &t.rows);
    if !owners.is_empty() && owners != &vec![vec![Cell::Integer(1), Cell::Text(account.into())]] {
        return Err(invalid("migration account mismatch"));
    }
    if protected && owners.is_empty() {
        return Err(invalid("protected account owner missing"));
    }
    keys_and_content(image)?;
    let rows = image
        .tables
        .get("composer_draft_sync")
        .map_or(&empty, |t| &t.rows);
    let kind = if protected {
        Class::Protected7
    } else if rows.is_empty() {
        Class::OrdinaryLocal
    } else {
        Class::Legacy5
    };
    for row in rows {
        let scope = Scope {
            account_id: account.into(),
            key: wire::strict(row[0].text()?)?,
        };
        let revision = row[1].integer()?;
        let version = row[2].integer()?;
        let raw = row[3].text()?;
        if kind == Class::Protected7 {
            let decoded = wire::decode(&scope, revision, version, raw)?;
            if wire::strict::<serde_json::Value>(raw)?
                != wire::strict::<serde_json::Value>(&wire::encode(&decoded)?)?
            {
                return Err(invalid("unknown protected authority fields"));
            }
        } else {
            if version != 5 {
                return Err(invalid("only exact legacy format 5 may migrate"));
            }
            let decoded =
                crate::drafts::store::sync::decode(&scope, revision, version, raw.to_owned())?;
            if wire::strict::<serde_json::Value>(raw)? != serde_json::to_value(&decoded.state)? {
                return Err(invalid("lossy legacy authority shape"));
            }
        }
    }
    // No unrecognized SQL has run: exact schema comparison preceded shadow().
    let mut shadow = image.shadow()?;
    if local_only {
        complete_local_schema(&shadow.0)?;
    }
    if kind == Class::Protected7 {
        loading::load(&shadow, account, LoadMode::Current, &BTreeMap::new())
            .map_err(|_| invalid("invalid protected inventory"))?;
    } else {
        shadow.load_synced(account, crate::drafts::store::sync::LoadMode::Current)?;
    }
    if uploads {
        shadow.load_uploads(account)?;
    }
    if slash {
        slash_intent::upgrade_audit(&shadow.0, account, binding, completed)?;
    }
    if hosted_cleanup {
        hosted_files::cleanup::all(&shadow.0, account)?;
    }
    if hosted_editor {
        hosted_files::editor::all(&shadow.0, account)?;
    }
    if hosted {
        hosted_files::retained(&shadow, account)
            .map_err(|_| invalid("invalid hosted inventory"))?;
    }
    if history {
        validate_history(image, account)?;
    }
    Ok(kind)
}
fn keys_and_content(image: &Image) -> Result<(), AppError> {
    for (name, table) in &image.tables {
        if let Some(index) = table.columns.iter().position(|s| s == "destination") {
            let mut spellings: BTreeMap<DraftKey, String> = BTreeMap::new();
            for row in &table.rows {
                let raw = row[index].text()?;
                let key: DraftKey = wire::strict(raw)?;
                if serde_json::to_string(&key)? != raw {
                    return Err(invalid("noncanonical destination spelling"));
                }
                if spellings
                    .insert(key, raw.into())
                    .is_some_and(|old| old != raw)
                {
                    return Err(invalid("duplicate semantic destination"));
                }
            }
        }
        if matches!(
            name.as_str(),
            "composer_drafts" | "composer_draft_blocked" | "composer_draft_alternates"
        ) {
            let content = table
                .columns
                .iter()
                .position(|s| s == "content")
                .ok_or_else(|| invalid("content column missing"))?;
            for row in &table.rows {
                let raw = row[content].text()?;
                let local: LocalDraft = wire::strict(raw)?;
                if wire::strict::<serde_json::Value>(raw)? != serde_json::to_value(local)? {
                    return Err(invalid("unknown local draft fields"));
                }
            }
        }
    }
    Ok(())
}
pub(super) fn validate_history(image: &Image, account: &str) -> Result<(), AppError> {
    let rows = &image.table("composer_draft_migration_receipt")?.rows;
    if rows.len() != 1
        || rows[0].len() != 6
        || rows[0][0] != Cell::Integer(1)
        || rows[0][1] != Cell::Integer(1)
        || rows[0][2].text()? != account
        || !valid_uuid(rows[0][3].text()?)
    {
        return Err(invalid("migration receipt identity changed"));
    }
    let source: Option<Image> = wire::strict(rows[0][4].text()?)?;
    let result: snapshot::Table = wire::strict(rows[0][5].text()?)?;
    result.shape()?;
    if result.columns != ["destination", "storage_revision", "format_version", "state"] {
        return Err(invalid("migration result shape changed"));
    }
    if let Some(source) = source {
        source.shape()?;
        if source
            .tables
            .contains_key("composer_draft_migration_receipt")
        {
            return Err(invalid("recursive migration history"));
        }
        // History is evidence only, not a freshness proof or dispatch permit.
        if !matches!(
            classify(&source, account)?,
            Class::OrdinaryLocal | Class::Legacy5
        ) {
            return Err(invalid("unsupported migration history source"));
        }
        validate_import(&source, &result, account)?;
    } else if !result.rows.is_empty() {
        return Err(invalid("missing-source migration has managed rows"));
    }
    Ok(())
}
pub(super) fn validate_import(
    before: &Image,
    after: &snapshot::Table,
    account: &str,
) -> Result<(), AppError> {
    before.shape()?;
    after.shape()?;
    if after.columns != ["destination", "storage_revision", "format_version", "state"] {
        return Err(invalid("migration output column shape"));
    }
    let Some(prior) = before.tables.get("composer_draft_sync") else {
        return if after.rows.is_empty() {
            Ok(())
        } else {
            Err(invalid("local-only import manufactured a managed row"))
        };
    };
    if prior.columns != after.columns || prior.rows.len() != after.rows.len() {
        return Err(invalid("migration row set changed"));
    }
    for (old, new) in prior.rows.iter().zip(&after.rows) {
        if old[0] != new[0]
            || old[2] != Cell::Integer(5)
            || new[2] != Cell::Integer(7)
            || old[1].integer()?.checked_add(1) != Some(new[1].integer()?)
        {
            return Err(invalid("migration revision or key changed"));
        }
        let scope = Scope {
            account_id: account.into(),
            key: wire::strict(old[0].text()?)?,
        };
        let parsed = wire::decode(&scope, new[1].integer()?, 7, new[3].text()?)?;
        if parsed.ownership.basis != super::super::super::Basis::Unverified
            || parsed.ownership.origin != super::super::super::Origin::Imported
            || parsed.ownership.verified_epoch != 0
            || wire::strict::<serde_json::Value>(old[3].text()?)?
                != serde_json::to_value(parsed.stored.state)?
        {
            return Err(invalid("migration changed state or issued authority"));
        }
    }
    Ok(())
}
pub(super) fn complete_local_schema(conn: &Connection) -> Result<(), AppError> {
    let ordinary = Image::read(&ordinary()?.0)?;
    for object in ordinary.schema {
        if object.kind == "table"
            && matches!(
                object.name.as_str(),
                "composer_draft_sync"
                    | "composer_draft_owner"
                    | "composer_draft_blocked"
                    | "composer_draft_alternates"
                    | "composer_draft_reviewed_writers"
            )
        {
            conn.execute_batch(
                object
                    .sql
                    .as_deref()
                    .ok_or_else(|| invalid("ordinary schema SQL missing"))?,
            )?;
        }
    }
    Ok(())
}
