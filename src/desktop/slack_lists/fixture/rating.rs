//! Static Rating cell/editor surface using the existing canonical fixture backend.
use super::*;
pub(crate) fn install(shell: &mut ShellState, name: &str) {
    assert!(matches!(
        name,
        "slack-list-rating-cells" | "slack-list-rating-cells-narrow"
    ));
    view_filter::seed(shell, name);
    let f = shell.slack_lists.fixture.as_mut().unwrap();
    f.document["list_metadata"]["schema"].as_array_mut().unwrap().push(json!({"id":"C_RATING","key":"rating","name":"Quality","type":"rating","options":{"emoji":":heart:","max":5}}));
    f.document["list_metadata"]["views"][0]["columns"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"C_RATING","key":"rating","visible":true}));
    for (index, row) in f.records["records"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        if index % 6 != 0 {
            row["fields"]
                .as_array_mut()
                .unwrap()
                .push(json!({"key":"rating","rating":[index%6]}));
        }
    }
    refresh(shell);
    let visit = generation(shell).unwrap();
    let row = shell.slack_lists.view.as_ref().unwrap().rows[0].id.clone();
    super::super::editor::begin_shell(shell, &row, "C_RATING", visit).unwrap();
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn named_rating_fixture_uses_actual_preflight_builder_completion_and_readback() {
        for alias in ["slack-list-rating-cells", "slack-list-rating-cells-narrow"] {
            let mut s = ShellState::fixture_variant(crate::media::MediaRegistry::default(), alias);
            assert!(s.core.session.is_none() && s.core.transport.is_none());
            let before = s.slack_lists.view.as_ref().unwrap().loaded.clone().unwrap();
            assert_eq!(before.records.records.len(), 73);
            let p = super::super::super::editor::presentation(&s).unwrap();
            let desired = if p.original.as_ref() == &CellEditValue::Rating(Some(5)) {
                Some(4)
            } else {
                Some(5)
            };
            let attempt = super::super::super::editor::begin_save(
                &mut s,
                p.generation,
                CellEditValue::Rating(desired),
            )
            .unwrap();
            dispatch(&mut s, attempt);
            assert_eq!(attempt_count(&s), 1);
            assert!(prepared_valid(&s));
            complete_pending(&mut s);
            assert_eq!(s.slack_lists.operations.len(), 1);
            assert_eq!(
                s.slack_lists
                    .view
                    .as_ref()
                    .unwrap()
                    .loaded
                    .as_ref()
                    .unwrap()
                    .records,
                before.records
            );
            refresh(&mut s);
            assert!(s.slack_lists.operations.is_empty());
            assert!(super::super::super::editor::presentation(&s).is_none());
            let after = s
                .slack_lists
                .view
                .as_ref()
                .unwrap()
                .loaded
                .as_ref()
                .unwrap();
            assert_eq!(after.document, before.document);
            assert_eq!(after.records.next_cursor, before.records.next_cursor);
            assert_eq!(after.records.records.len(), 73);
            let column = after.document.column("C_RATING").unwrap();
            let row = after
                .records
                .records
                .iter()
                .find(|r| r.id == p.row_id)
                .unwrap();
            assert_eq!(
                column
                    .edit_value(row, &after.document.id, after.personal_task_lists.as_ref())
                    .unwrap(),
                CellEditValue::Rating(desired)
            );
            complete_pending(&mut s);
            assert_eq!(attempt_count(&s), 1);
        }
    }
}
