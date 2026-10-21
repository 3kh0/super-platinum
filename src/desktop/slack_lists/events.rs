//! Known List events invalidate an open reader; server reads reconcile content.
use crate::state::ShellState;
use super_platinum_core::slack::events::RtEvent;

fn list_id(event: &RtEvent) -> Option<&str> {
    let RtEvent::Unknown(raw) = event else {
        return None;
    };
    let id = match raw.kind.as_str() {
        "list_record_removed"
        | "list_record_updated"
        | "list_record_created"
        | "list_records_archived"
        | "list_cells_updated"
        | "list_contents_merged" => raw.rest.get("list_id").and_then(|v| v.as_str()),
        "file_change" | "file_deleted" => {
            let top = raw.rest.get("file_id").and_then(|v| v.as_str());
            let nested = raw
                .rest
                .get("file")
                .and_then(|v| v.get("id"))
                .and_then(|v| v.as_str());
            if top.zip(nested).is_some_and(|(a, b)| a != b) {
                return None;
            }
            top.or(nested)
        }
        _ => None,
    };
    id.filter(|id| !id.trim().is_empty())
}

pub(crate) fn prepare(
    shell: &mut ShellState,
    team: &str,
    generation: u64,
    event: &RtEvent,
) -> bool {
    if shell.core.active_team.as_deref() != Some(team)
        || shell
            .core
            .workspaces
            .get(team)
            .is_none_or(|workspace| workspace.rt_generation != generation)
    {
        return false;
    }
    // Comments own a file conversation independent of the selected channel.
    // A reaction only reprojects the existing rows; it must not schedule a read.
    super::comments::own_edit::observe_event(shell, team, generation, event);
    super::comments::own_delete::observe_event(shell, team, generation, event);
    super::comments::apply_reaction(shell, team, generation, event);
    super::comments::apply_deletion(shell, team, generation, event);
    super::comments::apply_incoming(shell, team, generation, event);
    super::comments::apply_edit(shell, team, generation, event);
    super::comments::member_root::apply(shell, team, generation, event);
    let Some(id) = list_id(event) else {
        return false;
    };
    super::invalidate(shell, team, id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{media::MediaRegistry, slack_lists as lists};
    use serde_json::json;
    fn raw(value: serde_json::Value) -> RtEvent {
        RtEvent::Unknown(serde_json::from_value(value).unwrap())
    }
    #[test]
    fn supported_events_use_list_identity_not_record_or_column_identity() {
        for kind in [
            "list_record_removed",
            "list_record_updated",
            "list_record_created",
            "list_records_archived",
            "list_cells_updated",
            "list_contents_merged",
        ] {
            let event = raw(json!({"type":kind,"list_id":"F_LIST","id":"ROW","column_id":"COL"}));
            assert_eq!(list_id(&event), Some("F_LIST"));
            assert_eq!(list_id(&raw(json!({"type":kind,"id":"F_LIST"}))), None);
        }
        assert_eq!(
            list_id(&raw(json!({"type":"unrelated","list_id":"F_LIST"}))),
            None
        );
        assert_eq!(
            list_id(&raw(json!({"type":"list_cells_updated","list_id":" "}))),
            None
        );
    }
    #[test]
    fn file_events_require_consistent_file_ids() {
        assert_eq!(
            list_id(&raw(json!({"type":"file_change","file_id":"F_LIST"}))),
            Some("F_LIST")
        );
        assert_eq!(
            list_id(&raw(json!({"type":"file_deleted","file":{"id":"F_LIST"}}))),
            Some("F_LIST")
        );
        assert_eq!(
            list_id(&raw(
                json!({"type":"file_change","file_id":"F_OTHER","file":{"id":"F_LIST"}})
            )),
            None
        );
    }

    fn shell() -> ShellState {
        let mut shell = ShellState::fixture_variant(MediaRegistry::default(), "slack-list");
        shell.core.workspaces.get_mut("T1").unwrap().rt_generation = 7;
        shell
    }

    #[test]
    fn wrong_generation_team_and_list_leave_the_current_read_untouched() {
        let mut shell = shell();
        let request = lists::begin_read(&mut shell).unwrap();
        let revision = shell.slack_lists.view.as_ref().unwrap().revision;
        let event = raw(json!({"type":"list_cells_updated","list_id":"F_LIST_FIXTURE"}));
        for generation in [0, 6, 8] {
            assert!(!prepare(&mut shell, "T1", generation, &event));
        }
        assert!(!prepare(&mut shell, "T2", 7, &event));
        assert!(!prepare(&mut shell, "MISSING", 7, &event));
        let other = raw(json!({"type":"list_record_created","list_id":"F_OTHER"}));
        assert!(!prepare(&mut shell, "T1", 7, &other));
        assert!(request.current(&shell));
        let view = shell.slack_lists.view.as_ref().unwrap();
        assert_eq!(view.revision, revision);
        assert!(view.loading);
    }

    #[test]
    fn current_list_event_retires_prior_read_without_erasing_loaded_rows() {
        let mut shell = shell();
        let previous = lists::presentation(&shell).unwrap();
        let request = lists::begin_read(&mut shell).unwrap();
        let mut old = shell
            .slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .clone()
            .unwrap();
        old.document.title = "Stale response".into();
        let event = raw(json!({"type":"list_cells_updated","list_id":"F_LIST_FIXTURE"}));
        assert!(prepare(&mut shell, "T1", 7, &event));
        assert!(!request.current(&shell));
        assert!(lists::begin_read(&mut shell).is_none());
        assert!(lists::finish_read(&mut shell, &request, Ok(old)));
        let view = lists::presentation(&shell).unwrap();
        assert_eq!(view.title, previous.title);
        assert!(std::sync::Arc::ptr_eq(&view.rows, &previous.rows));
        assert!(view.stale);
        assert!(!view.loading);
        // Finishing the old read permits exactly one replacement; duplicate
        // callbacks from the old read cannot interfere with that replacement.
        assert!(lists::begin_read(&mut shell).is_some());
        assert!(!request.current(&shell));
    }

    #[test]
    fn file_deletion_blocks_refresh_and_edits_after_the_reducer_runs() {
        let mut shell = shell();
        let request = lists::begin_read(&mut shell).unwrap();
        let old = shell
            .slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .clone()
            .unwrap();
        let event = raw(json!({"type":"file_deleted","file_id":"F_LIST_FIXTURE"}));
        assert!(prepare(&mut shell, "T1", 7, &event));
        // realtime.rs dispatches prepare before the ordinary file reducer.
        shell.core.remove_file("T1", "F_LIST_FIXTURE");
        assert!(lists::begin_read(&mut shell).is_none());
        lists::finish_read(&mut shell, &request, Ok(old));
        assert!(lists::begin_edit(&mut shell, "R_LIST_000", "C_CHECK").is_err());
        let view = lists::presentation(&shell).unwrap();
        assert!(view.stale);
        assert!(view.read_only_reason.is_some());
        assert!(!view.loading);
    }

    #[test]
    fn account_change_rejects_event_before_it_can_touch_the_visible_list() {
        let mut shell = shell();
        let revision = shell.slack_lists.view.as_ref().unwrap().revision;
        shell.core.account_epoch += 1;
        let event = raw(json!({"type":"list_cells_updated","list_id":"F_LIST_FIXTURE"}));
        assert!(!prepare(&mut shell, "T1", 7, &event));
        assert_eq!(shell.slack_lists.view.as_ref().unwrap().revision, revision);
    }
}
