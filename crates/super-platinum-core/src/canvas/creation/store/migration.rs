use super::*;
/// Only after the bounded schema2 association audit succeeds, in the same transaction.
pub(super) fn upgrade(conn: &Connection) -> Result<()> {
    conn.execute_batch("CREATE TABLE retired_attempts(id TEXT PRIMARY KEY NOT NULL,body TEXT NOT NULL CHECK(json_valid(body) AND length(CAST(body AS BLOB))<=65536),owner_key TEXT GENERATED ALWAYS AS (json_extract(body,'$.owner.team')||'/'||json_extract(body,'$.owner.user')) STORED);
CREATE INDEX retired_attempt_owner ON retired_attempts(owner_key,id);
CREATE TABLE retired_decisions(id TEXT PRIMARY KEY NOT NULL,body TEXT NOT NULL CHECK(json_valid(body) AND length(CAST(body AS BLOB))<=65536));
CREATE TABLE evidence_dispositions(id TEXT PRIMARY KEY NOT NULL,body TEXT NOT NULL CHECK(json_valid(body) AND length(CAST(body AS BLOB))<=65536));
CREATE TABLE late_evidence(id TEXT PRIMARY KEY NOT NULL,body TEXT NOT NULL);
CREATE INDEX active_scope_ids ON scopes(id) WHERE json_extract(body,'$.active') IS NOT NULL;
PRAGMA user_version=3;")?;
    // Existing conflict dispositions stay byte-for-byte, including their original keys.
    Ok(())
}
