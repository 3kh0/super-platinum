//! Opt-in preflight diagnostics. Only fixed labels and booleans leave this module.
use super::*;
use serde_json::json;

const FILE_KEYS: &[&str] = &[
    "access",
    "channels",
    "comments_count",
    "created",
    "display_as_bot",
    "dm_mpdm_users_with_file_access",
    "edit_timestamp",
    "editable",
    "editors",
    "excluded_from_slack_ai",
    "external_type",
    "favorites",
    "file_access",
    "filetype",
    "groups",
    "has_more_shares",
    "has_rich_preview",
    "id",
    "ims",
    "is_ai_suggested",
    "is_external",
    "is_locked",
    "is_modified_by_ai",
    "is_public",
    "is_deleted",
    "is_tombstoned",
    "is_restricted_sharing_enabled",
    "is_starred",
    "last_editor",
    "last_read",
    "list_csv_download_url",
    "list_limits",
    "list_metadata",
    "mimetype",
    "mode",
    "name",
    "org_or_workspace_access",
    "permalink",
    "permalink_public",
    "pretty_type",
    "private_channels_with_file_access_count",
    "public_url_shared",
    "shares",
    "show_badge",
    "size",
    "teams_shared_with",
    "timestamp",
    "title",
    "updated",
    "url_private",
    "url_private_download",
    "user",
    "user_team",
    "username",
];
const RECORD_KEYS: &[&str] = &[
    "id",
    "list_id",
    "created_by",
    "date_created",
    "fields",
    "platform_refs",
    "position",
    "updated_by",
    "updated_timestamp",
    "view_positions",
    "parent_record_id",
    "is_archived",
    "is_deleted",
    "suggested",
];
const COLUMN_KEYS: &[&str] = &[
    "id",
    "key",
    "name",
    "type",
    "options",
    "is_primary_column",
    "is_deleted",
];
fn difference(old: Option<&Value>, new: Option<&Value>, allowlist: &[&str]) -> Value {
    let changed = allowlist
        .iter()
        .copied()
        .filter(|key| {
            old.and_then(|value| value.get(*key)) != new.and_then(|value| value.get(*key))
        })
        .collect::<Vec<_>>();
    let unknown_changed = old
        .into_iter()
        .chain(new)
        .filter_map(Value::as_object)
        .flat_map(|object| object.keys())
        .any(|key| {
            !allowlist.contains(&key.as_str())
                && old.and_then(|value| value.get(key)) != new.and_then(|value| value.get(key))
        });
    json!({"same":old==new,"old_present":old.is_some(),"new_present":new.is_some(),
        "changed_keys":changed,"other_keys_changed":unknown_changed})
}
pub(super) fn mismatch(shell: &ShellState, attempt: &EditAttempt, fresh: &Loaded) {
    if std::env::var_os("SUPER_PLATINUM_LIST_EDIT_TRACE").is_none() {
        return;
    }
    let previous = shell
        .slack_lists
        .view
        .as_ref()
        .and_then(|view| view.loaded.as_ref());
    let old_document = previous.map(|loaded| &loaded.document.raw);
    let old_record = previous
        .and_then(|loaded| loaded.records.record(attempt.edit.record_id()))
        .map(|record| &record.raw);
    let old_column = previous
        .and_then(|loaded| loaded.document.column(attempt.edit.column_id()))
        .map(|column| &column.raw);
    let record = fresh.records.record(attempt.edit.record_id());
    let column = fresh.document.column(attempt.edit.column_id());
    let report = json!({
        "list_edit":"preflight_mismatch",
        "list_id_same":fresh.document.id==attempt.edit.list_id(),
        "permission_ok":fresh.document.editable(fresh.allow_lists.as_deref()),
        "permission_same":previous.is_some_and(|old|old.allow_lists==fresh.allow_lists),
        "personal_policy_known":fresh.personal_task_lists.is_some(),
        "personal_policy_same":previous.is_some_and(|old|old.personal_task_lists==fresh.personal_task_lists),
        "ordinary_list":fresh.personal_task_lists.as_ref().is_some_and(|policy|!policy.contains(&fresh.document.id)),
        "cell_eligible":column.zip(record).is_some_and(|(column,record)|column.editable_value(record,&fresh.document.id,fresh.personal_task_lists.as_ref())),
        "document":difference(old_document,Some(&fresh.document.raw),FILE_KEYS),
        "record":difference(old_record,record.map(|record|&record.raw),RECORD_KEYS),
        "column":difference(old_column,column.map(|column|&column.raw),COLUMN_KEYS),
        "schema_same":old_document.and_then(|value|value.pointer("/list_metadata/schema"))==fresh.document.raw.pointer("/list_metadata/schema"),
        "views_same":old_document.and_then(|value|value.pointer("/list_metadata/views"))==fresh.document.raw.pointer("/list_metadata/views")
    });
    eprintln!("{report}");
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn diff_emits_only_allowlisted_key_names_never_values_or_unknown_names() {
        let old = json!({"last_read":"secret-before","list_metadata":{"secret-nested":"secret-value"},"secret-dynamic-key":false});
        let new = json!({"last_read":"secret-after","list_metadata":{"other-secret-nested":true},"secret-dynamic-key":true});
        let output = difference(Some(&old), Some(&new), FILE_KEYS);
        assert_eq!(
            output["changed_keys"],
            json!(["last_read", "list_metadata"])
        );
        assert_eq!(output["other_keys_changed"], true);
        assert_eq!(output["same"], false);
        assert!(!output.to_string().contains("secret"));
        assert_eq!(difference(Some(&old), Some(&old), FILE_KEYS)["same"], true);
    }
}
