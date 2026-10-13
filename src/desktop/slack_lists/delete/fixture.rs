//! Offline one-shot deletion backend. Completion is explicitly driven by the checker.
use super::*;
use serde_json::json;
use super_platinum_core::{
    config::WorkspaceSession,
    slack::{api, client::RequestBody},
};

#[derive(Clone, Default)]
pub(super) struct Fixture {
    pending: Option<Attempt>,
    read: Option<ReadbackAttempt>,
    attempts: usize,
    checks: usize,
    uncertain: bool,
    prepared_valid: bool,
    restore_attempts: usize,
    deleted: BTreeMap<String, Vec<Value>>,
}
pub(super) fn dispatch(shell: &mut ShellState, attempt: Attempt) {
    let Some(loaded) = super::super::fixture::deletion_loaded(shell) else {
        return;
    };
    let restored = if attempt.restoring {
        let response = if attempt.redo {
            json!({"ok":true,"record":loaded.records.record(attempt.intent.record_id()).unwrap().raw})
        } else {
            json!({"ok":false,"error":"record_deleted"})
        };
        let readback = ListRecordReadback::from_response(
            attempt.intent.list_id(),
            attempt.intent.record_id(),
            &response,
        )
        .unwrap();
        match undo::validate_restore(shell, &attempt, &loaded, &readback) {
            Ok(intent) => Some(intent),
            Err(error) => {
                rejected(shell, &attempt, error, None);
                return;
            }
        }
    } else {
        None
    };
    if !attempt.restoring
        && let Err(error) = validate(shell, &attempt, &loaded)
    {
        rejected(shell, &attempt, error, None);
        return;
    }
    let workspace = WorkspaceSession {
        team_id: "T_LIST_FIXTURE".into(),
        enterprise_id: None,
        user_id: "U_LIST_FIXTURE".into(),
        name: "Fixture".into(),
        url: "https://example.test".into(),
        token: "fixture-not-a-token".into(),
    };
    let request = if let Some(intent) = restored {
        api::list_record_update_deletion(&shell.core.client, &workspace, &intent)
    } else {
        api::list_record_delete(&shell.core.client, &workspace, &attempt.intent)
    };
    let reason = if attempt.restoring {
        "update-record-deletion-state"
    } else {
        "delete-record"
    };
    let path = if attempt.restoring {
        "/api/lists.records.updateDeletion"
    } else {
        "/api/lists.records.delete"
    };
    let valid = matches!(&request.body,RequestBody::Form(fields) if fields.iter().any(|(k,v)|k=="list_id"&&v==attempt.intent.list_id())&&fields.iter().any(|(k,v)|k=="id"&&v==attempt.intent.record_id())&&fields.iter().any(|(k,v)|k=="_x_reason"&&v==reason)&&(!attempt.restoring||fields.iter().any(|(k,v)|k=="deleted"&&v==if attempt.redo {"true"} else {"false"}))&&!fields.iter().any(|(k,_)|matches!(k.as_str(),"fields"|"cells")))
        && request.method == "POST"
        && !request.retry_safe()
        && url::Url::parse(&request.url).is_ok_and(|url| url.path() == path);
    let fixture = &mut shell.slack_lists.delete.fixture;
    fixture.prepared_valid = valid && (fixture.attempts == 0 || fixture.prepared_valid);
    if attempt.restoring {
        fixture.restore_attempts += 1;
    } else {
        fixture.attempts += 1;
        fixture.deleted.insert(
            attempt.intent.record_id().into(),
            loaded
                .records
                .records
                .iter()
                .filter(|record| {
                    record.id == attempt.intent.record_id()
                        || record.raw["parent_record_id"] == attempt.intent.record_id()
                })
                .map(|record| record.raw.clone())
                .collect(),
        );
    }
    fixture.pending = Some(attempt);
}
pub(super) fn check(shell: &mut ShellState, attempt: ReadbackAttempt) {
    if !super::super::fixture::enabled(shell) {
        return;
    }
    let fixture = &mut shell.slack_lists.delete.fixture;
    fixture.checks += 1;
    fixture.read = Some(attempt);
}
pub(crate) fn attempts(shell: &ShellState) -> usize {
    shell.slack_lists.delete.fixture.attempts
}
pub(crate) fn checks(shell: &ShellState) -> usize {
    shell.slack_lists.delete.fixture.checks
}
pub(crate) fn restore_attempts(shell: &ShellState) -> usize {
    shell.slack_lists.delete.fixture.restore_attempts
}
pub(crate) fn prepared_valid(shell: &ShellState) -> bool {
    shell.slack_lists.delete.fixture.prepared_valid
}
pub(crate) fn uncertain_next(shell: &mut ShellState) {
    if super::super::fixture::enabled(shell) {
        shell.slack_lists.delete.fixture.uncertain = true;
    }
}
pub(crate) fn complete(shell: &mut ShellState) -> bool {
    if !super::super::fixture::enabled(shell) {
        return false;
    }
    let Some(attempt) = shell.slack_lists.delete.fixture.pending.take() else {
        return false;
    };
    let uncertain = std::mem::take(&mut shell.slack_lists.delete.fixture.uncertain);
    if attempt.restoring {
        if !uncertain {
            if attempt.redo {
                super::super::fixture::deletion_remove(shell, attempt.intent.record_id());
            } else {
                restore_backend(shell, attempt.intent.record_id());
            }
        }
        let result = if uncertain {
            Err(ListDeleteFailure::Uncertain(Error::Transport(
                "fixture lost restore response".into(),
            )))
        } else {
            Ok(shell.slack_lists.delete.receipts[&attempt.key()]
                .restore_intent
                .as_ref()
                .unwrap()
                .validate_response(&json!({"ok":true}))
                .unwrap())
        };
        if lifecycle::finish_restore(shell, &attempt, result)
            && let Some(read) = begin_check(shell, attempt.serial)
        {
            check(shell, read);
        }
        return true;
    }
    if !uncertain {
        super::super::fixture::deletion_remove(shell, attempt.intent.record_id());
    }
    let result = if uncertain {
        Err(ListDeleteFailure::Uncertain(Error::Transport(
            "fixture lost response after dispatch".into(),
        )))
    } else {
        Ok(attempt
            .intent
            .validate_response(&json!({"ok":true}))
            .unwrap())
    };
    if finish(shell, &attempt, result)
        && let Some(read) = begin_check(shell, attempt.serial)
    {
        check(shell, read);
    }
    true
}
/// Return an explicitly requested synthetic tombstone or a present record. A
/// wrong-record response exercises identity refusal without changing the backend.
pub(crate) fn complete_check(shell: &mut ShellState, deleted: bool, wrong_record: bool) -> bool {
    if !super::super::fixture::enabled(shell) {
        return false;
    }
    let Some(read) = shell.slack_lists.delete.fixture.read.take() else {
        return false;
    };
    let id = if wrong_record {
        "R_LIST_OTHER"
    } else {
        read.record_id.as_str()
    };
    if !deleted
        && !wrong_record
        && shell
            .slack_lists
            .delete
            .receipts
            .get(&read.key)
            .is_some_and(|receipt| receipt.attempt.restoring && !receipt.attempt.redo)
    {
        restore_backend(shell, &read.record_id);
    }
    let response = if deleted {
        json!({"ok":false,"error":"record_deleted"})
    } else {
        let loaded = super::super::fixture::deletion_loaded(shell).unwrap();
        let Some(record) = loaded.records.records.iter().find(|record| record.id == id) else {
            return false;
        };
        json!({"ok":true,"record":record.raw})
    };
    let result = ListRecordReadback::from_response(&read.list_id, id, &response)
        .map_err(|error| Error::Api(error.to_string()));
    if deleted && !wrong_record {
        super::super::fixture::deletion_remove(shell, &read.record_id);
    }
    if finish_check(shell, &read, result) {
        super::super::fixture::refresh(shell);
    }
    true
}
fn restore_backend(shell: &mut ShellState, id: &str) {
    if let Some(records) = shell.slack_lists.delete.fixture.deleted.get(id).cloned() {
        super::super::fixture::deletion_restore(shell, &records);
    }
}
