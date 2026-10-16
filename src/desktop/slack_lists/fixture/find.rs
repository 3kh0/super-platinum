//! Static saved-view Find fixtures; no Slack session or transport.
use super::*;
pub(crate) fn install(shell: &mut ShellState, name: &str) {
    super::install(shell, name);
    super::add_saved_views(shell);
    let fixture = shell.slack_lists.fixture.as_mut().unwrap();
    // The dirty-draft check uses this one primary-name cell. Preserve its
    // existing fallback text while supplying the typed editor's exact seed.
    let row = fixture.records["records"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|row| row["id"] == "R_LIST_000")
        .unwrap();
    let field = row["fields"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|field| field["key"] == "NAME")
        .unwrap();
    let text = field["text"].as_str().unwrap().to_owned();
    field["rich_text"] = json!([{"type":"rich_text","elements":[{
        "type":"rich_text_section","elements":[{"type":"text","text":text}]
    }]}]);
    fixture.document["title"] = json!("Launch checklist · Find loaded items");
    // Hidden estimates must remain searchable; explicit incomplete metadata
    // stays visible even when Find leaves only one loaded match.
    fixture.document["list_metadata"]["default_view"] = json!("V_LIST_REORDERED");
    if name.contains("board") {
        fixture.document["list_metadata"]["views"][1]["type"] = json!("board");
        fixture.document["list_metadata"]["views"][1]["grouping"] = json!({"group_by":"status"});
        fixture.document["list_metadata"]["schema"][5]["options"]["choices"] = json!(
            (0..6)
                .map(|i| json!({"value":format!("OPT_FIND_{i}"),"label":format!("Stage {}",i+1)}))
                .collect::<Vec<_>>()
        );
        for (i, record) in fixture.records["records"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .enumerate()
        {
            let field = record["fields"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|field| field["key"] == "status")
                .unwrap();
            field["select"] = json!([format!("OPT_FIND_{}", i % 6)]);
        }
    }
    fixture.records["response_metadata"] = json!({"next_cursor":"opaque-find-fixture"});
    shell.slack_lists.view.as_mut().unwrap().layout = Default::default();
    super::refresh(shell);
    let view = shell.slack_lists.view.as_mut().unwrap();
    let text = if name.contains("empty") {
        "No such loaded item"
    } else if name.contains("board") {
        "Launch"
    } else {
        "72"
    };
    view.layout.find.input = text.into();
    view.layout.find.applied = text.into();
    let team = shell.slack_lists.scope.as_ref().unwrap().team.clone();
    super::super::layout::rebuild(view, shell.core.workspaces.get(&team));
}

pub(crate) fn check_enabled() -> bool {
    std::env::var("SUPER_PLATINUM_SLACK_LIST_FIND_CHECK").as_deref() == Ok("1")
        && matches!(
            std::env::var("SUPER_PLATINUM_FIXTURE").as_deref(),
            Ok("slack-list-find"
                | "slack-list-find-narrow"
                | "slack-list-find-board"
                | "slack-list-find-board-narrow")
        )
}
/// Bounded disconnected backend change; zero writes are prepared or dispatched.
pub(crate) fn change_backend(shell: &mut ShellState, stage: u8) -> Result<(), String> {
    if !super::enabled(shell) || stage > 2 {
        return Err("Invalid Find fixture backend change".into());
    }
    let fixture = shell.slack_lists.fixture.as_mut().unwrap();
    let row = fixture.records["records"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|row| row["id"] == "R_LIST_072")
        .ok_or("Missing Find row")?;
    let field = row["fields"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|field| field["key"] == "estimate")
        .ok_or("Missing Find number")?;
    field["number"] = json!([if stage == 0 { 72 } else { 888 }]);
    let column = fixture.document["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|column| column["id"] == "C_NAME")
        .ok_or("Missing Find schema")?;
    column["type"] = json!(if stage == 2 { "rich_text" } else { "text" });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_name_fixture_adds_only_the_exact_typed_editor_seed() {
        let mut ordinary =
            ShellState::fixture_variant(crate::media::MediaRegistry::default(), "slack-list");
        super::super::add_saved_views(&mut ordinary);
        let find =
            ShellState::fixture_variant(crate::media::MediaRegistry::default(), "slack-list-find");
        let mut records = find.slack_lists.fixture.as_ref().unwrap().records["records"].clone();
        let field = records
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|row| row["id"] == "R_LIST_000")
            .unwrap()["fields"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|field| field["key"] == "NAME")
            .unwrap();
        let rich = field.as_object_mut().unwrap().remove("rich_text").unwrap();
        assert_eq!(
            rich,
            json!([{"type":"rich_text","elements":[{
                "type":"rich_text_section","elements":[{"type":"text","text":"Launch item 01"}]
            }]}])
        );
        assert_eq!(
            records,
            ordinary.slack_lists.fixture.as_ref().unwrap().records["records"]
        );
        let loaded = find
            .slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap();
        assert_eq!(
            loaded
                .document
                .column("C_NAME")
                .unwrap()
                .edit_value(
                    loaded.records.record("R_LIST_000").unwrap(),
                    &loaded.document.id,
                    loaded.personal_task_lists.as_ref()
                )
                .unwrap(),
            super_platinum_core::slack_lists::CellEditValue::RichText(
                rich.as_array().unwrap().clone()
            )
        );
        let loaded = ordinary
            .slack_lists
            .view
            .as_ref()
            .unwrap()
            .loaded
            .as_ref()
            .unwrap();
        assert!(
            loaded
                .document
                .column("C_NAME")
                .unwrap()
                .edit_value(
                    loaded.records.record("R_LIST_000").unwrap(),
                    &loaded.document.id,
                    loaded.personal_task_lists.as_ref()
                )
                .is_err(),
            "Scalar-only original must remain refused by the production editor"
        );
    }
}
