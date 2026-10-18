//! AjOk nested execution projection. These helpers do not grant navigation:
//! the controller checks the original visit, document, filters and read/detail
//! generation before applying a returned breadcrumb stack.
use serde_json::Value;
use super_platinum_core::block_actions::workflow_management::ActivityPage;

/// Native retention bound, not a source pagination or execution-depth fact.
const MAX_DEPTH: usize = 64;

pub(super) fn rows<'a>(page: &'a ActivityPage, breadcrumbs: &'a [Value]) -> Vec<&'a Value> {
    // AjOk mapDataToWorkflowEvents: an empty child page stays empty.
    breadcrumbs
        .last()
        .filter(|_| !page.rows().is_empty())
        .into_iter()
        .chain(page.rows().iter())
        .collect()
}

pub(super) fn is_parent(breadcrumbs: &[Value], row: &Value) -> bool {
    breadcrumbs
        .last()
        .is_some_and(|parent| parent["workflow_execution_id"] == row["workflow_execution_id"])
}

/// `observed_rows` is the current joined page, or the original selected detail
/// row after the controller has checked its independent detail revision.
pub(super) fn child(
    breadcrumbs: &[Value],
    observed_rows: &[&Value],
    row: &Value,
) -> Option<Vec<Value>> {
    if !observed_rows.contains(&row)
        || is_parent(breadcrumbs, row)
        || breadcrumbs.len() >= MAX_DEPTH
        || row["workflow_execution_id"]
            .as_str()
            .is_none_or(str::is_empty)
        || row["child_execution_details"]["total_count"]
            .as_u64()
            .is_none_or(|count| count == 0)
    {
        return None;
    }
    let mut next = breadcrumbs.to_vec();
    next.push(row.clone());
    Some(next)
}

/// A breadcrumb callback carries the whole original stack, not just a depth.
pub(super) fn depth(current: &[Value], expected: &[Value], depth: usize) -> Option<Vec<Value>> {
    (current == expected && depth < current.len()).then(|| current[..depth].to_vec())
}

/// The detail panel may navigate to the prior parent even after its list page
/// changes. The selected detail's revision/row remains a controller guard.
pub(super) fn prior(current: &[Value], expected: &[Value], selected: &Value) -> Option<Vec<Value>> {
    if is_parent(current, selected) {
        return None;
    }
    depth(current, expected, expected.len().checked_sub(1)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use super_platinum_core::block_actions::workflow_management::{ActivityFilters, ActivityQuery};

    fn row(id: &str) -> Value {
        json!({"workflow_execution_id":id,"workflow_id":"WF_REVIEW",
            "child_execution_details":{"total_count":2},"opaque":{"revision":73}})
    }

    #[test]
    fn parent_is_only_joined_to_nonempty_children_without_rewriting_rows() {
        let query = ActivityQuery::new("WF_REVIEW", ActivityFilters::default())
            .unwrap()
            .with_parent(Some("PARENT"))
            .unwrap();
        let breadcrumbs = vec![row("PARENT")];
        let empty = ActivityPage::decode(&query, json!({"ok":true,"workflow_events":[]})).unwrap();
        assert!(rows(&empty, &breadcrumbs).is_empty());
        let child = row("CHILD");
        let page =
            ActivityPage::decode(&query, json!({"ok":true,"workflow_events":[child]})).unwrap();
        assert_eq!(rows(&page, &breadcrumbs), vec![&breadcrumbs[0], &child]);
        assert_eq!(page.query().parent_execution_id(), Some("PARENT"));
    }

    #[test]
    fn child_requires_the_exact_original_row_and_retains_opaque_fields() {
        let selected = row("PARENT");
        let next = child(&[], &[&selected], &selected).unwrap();
        assert_eq!(next, vec![selected.clone()]);
        let mut changed = selected.clone();
        changed["opaque"]["revision"] = json!(74);
        assert!(child(&[], &[&changed], &selected).is_none());
        assert!(child(&next, &[&selected], &selected).is_none());
        changed["child_execution_details"]["total_count"] = json!(0);
        assert!(child(&[], &[&changed], &changed).is_none());
        assert!(child(&[], &[], &selected).is_none());
    }

    #[test]
    fn breadcrumb_callback_cannot_address_replacement_stack_at_same_depth() {
        let stack = vec![row("PARENT"), row("NESTED")];
        assert_eq!(depth(&stack, &stack, 1), Some(vec![stack[0].clone()]));
        assert_eq!(depth(&stack, &stack, 0), Some(vec![]));
        assert!(depth(&stack, &[row("OTHER"), row("NESTED")], 1).is_none());
        assert!(depth(&stack, &stack, 2).is_none());
        assert!(depth(&[], &[], 0).is_none());
    }

    #[test]
    fn selected_detail_can_use_original_row_independently_of_current_page() {
        let selected = row("SELECTED");
        let replacement = row("OTHER");
        assert!(child(&[], &[&replacement], &selected).is_none());
        assert_eq!(
            child(&[], &[&selected], &selected),
            Some(vec![selected.clone()])
        );
        let stack = vec![row("PARENT")];
        assert_eq!(prior(&stack, &stack, &selected), Some(vec![]));
        assert!(prior(&stack, &stack, &stack[0]).is_none());
        assert!(prior(&[], &[], &selected).is_none());
    }
}
