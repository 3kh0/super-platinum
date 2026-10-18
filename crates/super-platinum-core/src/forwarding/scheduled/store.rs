//! Blocking SQLite custody. Invoke from a blocking worker, never render/event code.
//! No opening, read, expiry or restart dispatches a Slack request.
use super::*;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::{fs::OpenOptions, path::Path, time::Duration};

pub struct Journal {
    conn: Connection,
}
fn account_id(account: &str) -> Result<String, AppError> {
    uuid::Uuid::parse_str(account)
        .map(|v| v.to_string())
        .map_err(|_| invalid("Invalid scheduled-forward account"))
}
fn stamp(now: u64) -> Result<i64, AppError> {
    i64::try_from(now).map_err(|_| invalid("Invalid scheduled-forward clock"))
}
fn load(conn: &Connection, key: &str) -> Result<Option<Record>, AppError> {
    let text: Option<String> = conn
        .query_row("SELECT body FROM forwards WHERE id=?1", [key], |r| r.get(0))
        .optional()?;
    text.map(|text| {
        let r: Record = serde_json::from_str(&text)?;
        r.validate()?;
        Ok(r)
    })
    .transpose()
}
fn put(conn: &Connection, record: &Record) -> Result<(), AppError> {
    record.validate()?;
    let revision = i64::try_from(record.revision)
        .map_err(|_| invalid("Scheduled-forward revision exceeds SQLite integer range"))?;
    let body = serde_json::to_string(record)?;
    conn.execute(
        "INSERT INTO versions(id,revision,body) VALUES(?1,?2,?3)",
        params![record.id, revision, body],
    )?;
    conn.execute("INSERT INTO forwards(id,team,user,updated_at,body) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET updated_at=excluded.updated_at,body=excluded.body",params![record.id,record.intent.owner.team,record.intent.owner.user,stamp(record.updated_at)?,body])?;
    Ok(())
}
fn exact(conn: &Connection, expected: &Record) -> Result<Record, AppError> {
    let current =
        load(conn, &expected.id)?.ok_or_else(|| invalid("Scheduled forward no longer retained"))?;
    if current != *expected {
        return Err(invalid(
            "Scheduled forward changed; review the retained record",
        ));
    }
    Ok(current)
}
fn advance(record: &mut Record, now: u64) -> Result<(), AppError> {
    stamp(now)?;
    record.revision = record
        .revision
        .checked_add(1)
        .ok_or_else(|| invalid("Scheduled-forward revision exhausted"))?;
    record.updated_at = now;
    Ok(())
}

impl Journal {
    pub fn open_default(account: &str) -> Result<Self, AppError> {
        Self::open(
            crate::config::config_dir()?.join(format!(
                "scheduled-forwards-{}.sqlite",
                account_id(account)?
            )),
            account,
        )
    }
    /// Fixtures supply a private explicit path; another account cannot adopt it.
    pub fn open(path: impl AsRef<Path>, account: &str) -> Result<Self, AppError> {
        let account = account_id(account)?;
        let path = path.as_ref();
        if let Some(parent) = path.parent().filter(|v| !v.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        }
        drop(file);
        let mut conn = Connection::open(path)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.execute_batch("PRAGMA synchronous=FULL; PRAGMA journal_mode=DELETE;")?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let version: i64 = tx.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if ![0, 1].contains(&version) {
            return Err(invalid("Unsupported scheduled-forward journal"));
        }
        tx.execute_batch("CREATE TABLE IF NOT EXISTS identity(singleton INTEGER PRIMARY KEY CHECK(singleton=1),account TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS forwards(id TEXT PRIMARY KEY,team TEXT NOT NULL,user TEXT NOT NULL,updated_at INTEGER NOT NULL,body TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS versions(id TEXT NOT NULL,revision INTEGER NOT NULL,body TEXT NOT NULL,PRIMARY KEY(id,revision));
          CREATE TABLE IF NOT EXISTS outcomes(attempt TEXT PRIMARY KEY,id TEXT NOT NULL,body TEXT NOT NULL);")?;
        let prior: Option<String> = tx
            .query_row("SELECT account FROM identity WHERE singleton=1", [], |r| {
                r.get(0)
            })
            .optional()?;
        match prior {
            Some(prior) if prior != account => {
                return Err(invalid(
                    "Scheduled-forward journal belongs to another account",
                ));
            }
            None => {
                tx.execute(
                    "INSERT INTO identity(singleton,account) VALUES(1,?1)",
                    [account],
                )?;
            }
            _ => {}
        }
        tx.execute_batch("PRAGMA user_version=1;")?;
        tx.commit()?;
        Ok(Self { conn })
    }
    pub fn create(&mut self, intent: Intent, now: u64) -> Result<Record, AppError> {
        intent.validate()?;
        stamp(now)?;
        let record = Record {
            id: intent.client_msg_id.clone(),
            revision: 0,
            intent,
            phase: Phase::Ready,
            action: Action::Schedule,
            attempt: None,
            response_recorded: false,
            retry_at: 0,
            updated_at: now,
            reviewed: None,
            observed: None,
            outcome: String::new(),
        };
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(existing) = load(&tx, &record.id)? {
            if existing.intent != record.intent {
                return Err(invalid(
                    "This scheduled-forward identity already has different input",
                ));
            }
            tx.commit()?;
            return Ok(existing);
        }
        put(&tx, &record)?;
        tx.commit()?;
        Ok(record)
    }
    pub fn get(&self, key: &str) -> Result<Option<Record>, AppError> {
        load(&self.conn, key)
    }
    /// Persist Retry-After for read-only reconciliation too. Expiry does not send.
    pub fn defer_review(
        &mut self,
        expected: &Record,
        seconds: u64,
        now: u64,
    ) -> Result<Record, AppError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut r = exact(&tx, expected)?;
        if !matches!(r.phase, Phase::Unknown | Phase::Conflict) {
            return Err(invalid("No pending scheduled-forward review"));
        }
        r.retry_at = r.retry_at.max(now.saturating_add(seconds));
        advance(&mut r, now)?;
        put(&tx, &r)?;
        tx.commit()?;
        Ok(r)
    }
    /// Stable cursor paging, no arbitrary total-history cutoff or unresolved pruning.
    pub fn page(&self, owner: &Owner, after: Option<&str>) -> Result<Vec<Record>, AppError> {
        let mut stmt = self.conn.prepare(
            "SELECT body FROM forwards WHERE team=?1 AND user=?2 AND id>?3 ORDER BY id LIMIT 50",
        )?;
        let rows = stmt.query_map(params![owner.team, owner.user, after.unwrap_or("")], |r| {
            r.get::<_, String>(0)
        })?;
        rows.map(|row| {
            let record: Record = serde_json::from_str(&row?)?;
            record.validate()?;
            Ok(record)
        })
        .collect()
    }
    /// Undispatched input save or exact reviewed edit. Full raw equality is a
    /// local stale-state guard; it is not claimed to be a server compare-and-swap.
    pub fn revise(
        &mut self,
        expected: &Record,
        note: Vec<Value>,
        post_at: i64,
        preflight: Option<&Value>,
        now: u64,
    ) -> Result<Record, AppError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut r = exact(&tx, expected)?;
        if !matches!(r.phase, Phase::Ready | Phase::Rejected | Phase::Confirmed) {
            return Err(invalid(
                "Review the uncertain scheduled forward before editing",
            ));
        }
        if let Some(base) = r.reviewed.as_ref() {
            let Some(raw) = preflight else {
                return Err(invalid("Read the scheduled draft before editing"));
            };
            if raw.get("draft") != base.get("draft")
                || !request::complete_for(&r, raw)
                || raw["draft"]["is_deleted"] != false
                || raw["draft"]["is_sent"] != false
            {
                return Err(invalid("The scheduled draft changed; retain both versions"));
            }
        } else if preflight.is_some() || post_at == 0 {
            return Err(invalid("No reviewed server draft"));
        }
        if post_at != 0 {
            crate::scheduled::validate_time(
                post_at,
                i64::try_from(now).map_err(|_| invalid("Invalid clock"))?,
            )
            .map_err(invalid)?;
        }
        r.intent.blocks = append_reference(&note, &r.intent.source)?;
        r.intent.note = note;
        r.intent.post_at = post_at;
        r.action = if post_at == 0 {
            Action::SaveAsDraft
        } else {
            Action::Schedule
        };
        r.phase = Phase::Ready;
        r.outcome.clear();
        r.observed = None;
        advance(&mut r, now)?;
        put(&tx, &r)?;
        tx.commit()?;
        Ok(r)
    }
    /// The caller obtains fresh exact readback for an existing draft immediately
    /// before reserve, then rechecks its captured account/owner before dispatch.
    pub fn reserve(
        &mut self,
        expected: &Record,
        delete: bool,
        preflight: Option<&Value>,
        now: u64,
    ) -> Result<Attempt, AppError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut r = exact(&tx, expected)?;
        if now < r.retry_at
            || !matches!(r.phase, Phase::Ready | Phase::Rejected)
                && !(delete && r.phase == Phase::Confirmed)
        {
            return Err(invalid("Scheduled forward is retained or cooling down"));
        }
        if let Some(base) = r.reviewed.as_ref() {
            let Some(raw) = preflight else {
                return Err(invalid("Fresh draft review required"));
            };
            if raw.get("draft") != base.get("draft")
                || !request::complete_for(&r, raw)
                || raw["draft"]["is_deleted"] != false
                || raw["draft"]["is_sent"] != false
            {
                return Err(invalid("The reviewed draft changed"));
            }
        } else if delete || preflight.is_some() {
            return Err(invalid("No reviewed server draft"));
        }
        if !delete && r.intent.post_at != 0 {
            crate::scheduled::validate_time(
                r.intent.post_at,
                i64::try_from(now).map_err(|_| invalid("Invalid clock"))?,
            )
            .map_err(invalid)?;
        }
        if delete {
            r.action = Action::Delete;
        }
        r.phase = Phase::Unknown;
        r.attempt = Some(uuid::Uuid::new_v4().to_string());
        r.response_recorded = false;
        r.outcome="The scheduled-forward outcome is not confirmed. Check its retained draft before another write.".into();
        advance(&mut r, now)?;
        let now_ms = now
            .checked_mul(1000)
            .ok_or_else(|| invalid("Invalid mutation clock"))?;
        put(&tx, &r)?;
        tx.commit()?;
        Ok(Attempt {
            record: r,
            request_taken: false,
            client_last_updated_ts: now_ms,
        })
    }
    /// Exact undispatched capability only. An already taken request cannot use this.
    pub fn abandon(&mut self, attempt: Attempt, now: u64) -> Result<Record, AppError> {
        if attempt.request_taken {
            return Err(invalid("A prepared request may have reached Slack"));
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut r = exact(&tx, &attempt.record)?;
        r.phase = Phase::Ready;
        r.outcome = "No request was prepared; your input is retained.".into();
        r.response_recorded = true;
        advance(&mut r, now)?;
        put(&tx, &r)?;
        tx.commit()?;
        Ok(r)
    }
    /// Retain the original response even if a later read/visit has replaced the
    /// current record. A late callback never overwrites a newer attempt.
    pub fn finish(&mut self, completion: &Completion) -> Result<Record, AppError> {
        let attempt = &completion.attempt;
        let result = &completion.result;
        let now = completion.completed_at;
        stamp(now)?;
        if !attempt.request_taken {
            return Err(invalid("No prepared scheduled-forward request"));
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut r = load(&tx, &attempt.record.id)?
            .ok_or_else(|| invalid("Missing retained scheduled forward"))?;
        let mut phase = Phase::Unknown;
        let mut retry_at = 0;
        let evidence = match result {
            Ok(raw) => {
                if request::matching(&attempt.record, raw) {
                    phase = if attempt.record.action == Action::Delete {
                        Phase::Deleted
                    } else {
                        Phase::Confirmed
                    };
                } else if crate::scheduled::edit::definite_rejection(raw).is_some() {
                    phase = Phase::Rejected;
                }
                json!({"response":raw})
            }
            Err(crate::slack::Error::RateLimited { retry_after_secs }) => {
                phase = Phase::Rejected;
                retry_at = now.saturating_add(retry_after_secs.unwrap_or(30));
                json!({"failure":"rate_limited","retry_at":retry_at})
            }
            Err(crate::slack::Error::Api(code)) if code == "draft_mutation_not_sent_offline" => {
                phase = Phase::Rejected;
                json!({"failure":"not_sent_offline"})
            }
            Err(_) => json!({"failure":"outcome_unknown"}),
        };
        let key = attempt
            .record
            .attempt
            .as_deref()
            .ok_or_else(|| invalid("Missing attempt identity"))?;
        tx.execute(
            "INSERT OR IGNORE INTO outcomes(attempt,id,body) VALUES(?1,?2,?3)",
            params![key, r.id, serde_json::to_string(&evidence)?],
        )?;
        if r.attempt == attempt.record.attempt && !r.response_recorded {
            r.response_recorded = true;
            r.retry_at = r.retry_at.max(retry_at);
            // A complete successful read may already have confirmed this attempt.
            if r.phase == Phase::Unknown {
                r.phase = phase;
                if let Some(raw) = evidence.get("response") {
                    r.observed = Some(raw.clone());
                    if matches!(phase, Phase::Confirmed | Phase::Deleted) {
                        r.reviewed = Some(raw.clone());
                    }
                }
                r.outcome=match phase{Phase::Confirmed=>"The complete scheduled draft was confirmed.",Phase::Deleted=>"The complete deleted draft was confirmed.",Phase::Rejected=>"Slack did not accept this attempt. Your complete input is retained for an explicit retry.",_=>"The outcome is unknown. Check the retained draft; nothing will be resent automatically."}.into();
            }
            advance(&mut r, now)?;
            put(&tx, &r)?;
        }
        tx.commit()?;
        Ok(r)
    }
    pub fn review(
        &mut self,
        expected: &Record,
        response: &Value,
        now: u64,
    ) -> Result<Record, AppError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut r = exact(&tx, expected)?;
        if !matches!(r.phase, Phase::Unknown | Phase::Conflict) || now < r.retry_at {
            return Err(invalid("This scheduled forward is not ready for review"));
        }
        if !request::complete_for(&r, response) {
            return Err(invalid(
                "Readback did not return a complete matching draft identity",
            ));
        }
        r.observed = Some(response.clone());
        if request::matching(&r, response) {
            r.phase = if r.action == Action::Delete {
                Phase::Deleted
            } else {
                Phase::Confirmed
            };
            r.reviewed = Some(response.clone());
            r.outcome = "The complete retained draft now matches the requested state.".into();
        } else {
            r.phase = Phase::Conflict;
            r.outcome="Slack returned a different draft. Both your requested input and the observed version are retained; no write was retried.".into();
        }
        advance(&mut r, now)?;
        put(&tx, &r)?;
        tx.commit()?;
        Ok(r)
    }
}

mod recovery;
mod resolution;

#[cfg(test)]
mod revision_tests {
    use super::*;

    #[test]
    fn sqlite_revision_max_round_trips_and_exhaustion_keeps_the_original_record() {
        let directory =
            std::env::temp_dir().join(format!("scheduled-revision-{}", uuid::Uuid::new_v4()));
        let path = directory.join("private.sqlite");
        let account = uuid::Uuid::new_v4().to_string();
        {
            let mut journal = Journal::open(&path, &account).unwrap();
            let intent = Intent::new(
                Owner {
                    team: "T1".into(),
                    user: "U1".into(),
                },
                Source {
                    channel: "C1".into(),
                    timestamp: "100.000001".into(),
                    thread: None,
                    author: Some("U1".into()),
                    permalink: "https://fixture.slack.com/archives/C1/p100000001".into(),
                    private: false,
                },
                Destination::Conversation("C2".into()),
                "Destination".into(),
                vec![],
                2000,
                false,
                1000,
            )
            .unwrap();
            let mut record = journal.create(intent, 1000).unwrap();
            record.revision = u64::try_from(i64::MAX).unwrap();
            put(&journal.conn, &record).unwrap();
            assert_eq!(journal.get(record.id()).unwrap(), Some(record.clone()));
            let (stored, storage_type):(i64,String) = journal.conn.query_row(
                "SELECT revision,typeof(revision) FROM versions WHERE id=?1 ORDER BY revision DESC LIMIT 1",
                [record.id()], |row|Ok((row.get(0)?,row.get(1)?)),
            ).unwrap();
            assert_eq!(stored, i64::MAX);
            assert_eq!(storage_type, "integer");
            let reopened = Journal::open(&path, &account).unwrap();
            assert_eq!(reopened.get(record.id()).unwrap(), Some(record.clone()));
            drop(reopened);
            assert!(journal.reserve(&record, false, None, 1001).is_err());
            assert_eq!(journal.get(record.id()).unwrap(), Some(record.clone()));
            let mut outside = record.clone();
            outside.revision = record.revision.checked_add(1).unwrap();
            assert!(put(&journal.conn, &outside).is_err());
            outside.revision = u64::MAX;
            assert!(advance(&mut outside, 1002).is_err());
            assert!(put(&journal.conn, &outside).is_err());
            assert_eq!(journal.get(record.id()).unwrap(), Some(record));
            let count: i64 = journal
                .conn
                .query_row("SELECT count(*) FROM versions", [], |row| row.get(0))
                .unwrap();
            assert_eq!(
                count, 2,
                "overflow must not add a version or durable reservation"
            );
        }
        std::fs::remove_dir_all(directory).unwrap();
    }
}
