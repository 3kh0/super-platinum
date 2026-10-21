//! Named closed permission changes and an observable rate-limit window.
use super::*;

fn permission_case(case: &str) -> bool {
    case == "own-delete-guards"
}

pub(super) fn retry_after(case: &str) -> u64 {
    if case == "own-delete-rate-limit" {
        15
    } else {
        2
    }
}

pub(crate) fn permission(shell: &ShellState, allowed: bool) -> Result<Value, String> {
    let case = std::env::var("SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE").unwrap_or_default();
    if !permission_case(&case) {
        return Err("Exact deletion permission guard fixture required".into());
    }
    let model = model(shell).map_err(|_| "Closed deletion fixture unavailable")?;
    let session = session(shell).map_err(|_| "Original fixture identity missing")?;
    let entry = shell
        .slack_lists
        .comments
        .entry
        .as_ref()
        .ok_or("Original comments missing")?;
    if !entry.owner.active(shell)
        || session.team_id != "T1"
        || session.user_id != completion::SELF
        || entry.target.channel_id() != "CCOMMENT01"
        || entry.target.list_id() != LIST
        || entry.target.record_id() != "R_LIST_000"
        || entry.target.root_ts() != Some(ROOT)
    {
        return Err("Original permission fixture scope retired".into());
    }
    let mut ledger = model
        .0
        .lock()
        .map_err(|_| "Delete fixture ledger poisoned")?;
    if entry.target.document_raw() != &ledger.document
        || ledger.records.get("R_LIST_000") != Some(entry.target.record_raw())
    {
        return Err("Original permission fixture source changed".into());
    }
    let ledger = ledger
        .own_delete
        .as_mut()
        .ok_or("Delete fixture not seeded")?;
    let before = !ledger.permission_revoked;
    ledger.permission_revoked = !allowed;
    Ok(json!({"allowedBefore":before,"allowedAfter":allowed,
        "response":policy::permission_response(allowed),"workspace":session.team_id,
        "user":session.user_id,"channel":"CCOMMENT01","fixtureOnly":true}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn permission_mutation_is_exact_case_only() {
        assert!(permission_case("own-delete-guards"));
        for case in [
            "own-delete",
            "own-delete-rate-limit",
            "own-delete-guards-extra",
            "own-delete-restart-unknown-restore",
            "",
        ] {
            assert!(!permission_case(case));
        }
        assert!(!State::default().permission_revoked);
    }
    #[test]
    fn long_retry_window_is_exact_rate_case_only() {
        assert_eq!(retry_after("own-delete-rate-limit"), 15);
        for case in [
            "own-delete",
            "own-delete-failure",
            "own-delete-guards",
            "own-delete-rate-limit-extra",
            "",
        ] {
            assert_eq!(retry_after(case), 2);
        }
    }
}
