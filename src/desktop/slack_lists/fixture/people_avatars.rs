//! Disconnected display parity: ready, pending, missing and unknown People.
use super::*;
pub(crate) fn install(shell: &mut ShellState, name: &str) {
    super::install(shell, "slack-list");
    super::add_user_edits(shell).unwrap();
    let w = shell.core.workspaces.get_mut("T1").unwrap();
    w.users
        .get_mut("U00000001")
        .unwrap()
        .profile
        .as_mut()
        .unwrap()
        .image_48 = Some("https://avatar.invalid/ready.png".into());
    w.users
        .get_mut("U00000002")
        .unwrap()
        .profile
        .as_mut()
        .unwrap()
        .image_48 = Some("https://avatar.invalid/pending.png".into());
    let ready = shell
        .media
        .register_avatar("U00000001", "https://avatar.invalid/ready.png");
    shell.media.insert(
        ready,
        "image/png",
        include_bytes!("../../../../assets/icons/icon-256.png").as_slice(),
    );
    let f = shell.slack_lists.fixture.as_mut().unwrap();
    for column in f.document["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap()
    {
        if column["type"] == "user" {
            column["options"]["show_member_name"] = json!(false);
        }
    }
    // Keep these fields near the top of the real full-schema detail surface.
    f.document["list_metadata"]["schema"]
        .as_array_mut()
        .unwrap()
        .sort_by_key(|c| match c["id"].as_str() {
            Some("C_NAME") => 0,
            Some("C_EDIT_USER") => 1,
            Some("C_EDIT_USERS") => 2,
            Some("C_EDIT_TASK_USER") => 3,
            _ => 4,
        });
    let row = f.records["records"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|r| r["id"] == "R_LIST_000")
        .unwrap();
    let multiple = row["fields"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|c| c["key"] == "edit_users")
        .unwrap();
    multiple["user"] = json!(["U00000001", "U00000002", "W00000003", "U99999999"]);
    let view = f.document["list_metadata"]["views"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|v| v["id"] == "V_EDIT_USERS")
        .unwrap();
    view["type"] = json!(if name.contains("board") {
        "board"
    } else {
        "table"
    });
    view["name"] = json!("People avatars");
    // Board and detail remain real saved-view/record surfaces, not fake cells.
    super::refresh(shell);
    let visit = super::super::generation(shell).unwrap();
    let generation = shell.slack_lists.view.as_ref().unwrap().layout.generation;
    assert!(super::super::layout::select_shell(
        shell,
        Some("V_EDIT_USERS".into()),
        visit,
        generation
    ));
    if name.contains("detail") {
        let v = shell.slack_lists.view.as_ref().unwrap();
        let instance = v
            .rows
            .iter()
            .find(|r| r.id == "R_LIST_000")
            .unwrap()
            .instance_id
            .clone();
        let generation = v.layout.generation;
        assert!(super::super::record_detail::open_shell(
            shell, &instance, visit, generation
        ));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_six_surfaces_keep_canonical_people_and_disconnected_ownership() {
        for name in [
            "slack-list-people-avatars",
            "slack-list-people-avatars-narrow",
            "slack-list-people-avatars-board",
            "slack-list-people-avatars-board-narrow",
            "slack-list-people-avatars-detail",
            "slack-list-people-avatars-detail-narrow",
        ] {
            let s = ShellState::fixture_variant(crate::media::MediaRegistry::default(), name);
            assert!(enabled(&s));
            assert_eq!(attempt_count(&s), 0);
            let v = s.slack_lists.view.as_ref().unwrap();
            assert_eq!(v.loaded.as_ref().unwrap().records.records.len(), 73);
            assert_eq!(v.detail.is_some(), name.contains("detail"));
            assert_eq!(
                crate::slack_lists::presentation(&s).unwrap().layout_kind
                    == super_platinum_core::slack_lists::saved_views::ProjectionLayout::Board,
                name.contains("board")
            );
            let row = v.all_rows.iter().find(|r| r.id == "R_LIST_000").unwrap();
            let c = row
                .cells
                .iter()
                .find(|c| c.column_id == "C_EDIT_USERS")
                .unwrap();
            let CellContent::People(people) = &c.content else {
                panic!("{name}")
            };
            assert_eq!(people.len(), 4);
            assert!(c.text.contains("@Alice Example"));
            assert!(c.text.contains("U99999999"));
            assert!(matches!(
                row.cells
                    .iter()
                    .find(|c| c.column_id == "C_EDIT_TASK_USER")
                    .unwrap()
                    .content,
                CellContent::Rich(_)
            ));
        }
    }
    #[test]
    fn actual_show_names_readback_reprojects_content_without_changing_text_or_records() {
        let mut s = ShellState::fixture_variant(
            crate::media::MediaRegistry::default(),
            "slack-list-people-avatars",
        );
        let row = |s: &ShellState| {
            s.slack_lists
                .view
                .as_ref()
                .unwrap()
                .all_rows
                .iter()
                .find(|r| r.id == "R_LIST_000")
                .unwrap()
                .clone()
        };
        let before = row(&s);
        let text = before
            .cells
            .iter()
            .find(|c| c.column_id == "C_EDIT_USERS")
            .unwrap()
            .text
            .clone();
        let records = s.slack_lists.fixture.as_ref().unwrap().records.clone();
        for flag in [true, false] {
            let f = s.slack_lists.fixture.as_mut().unwrap();
            f.document["list_metadata"]["schema"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|c| c["id"] == "C_EDIT_USERS")
                .unwrap()["options"]["show_member_name"] = json!(flag);
            crate::slack_lists::fixture::refresh(&mut s);
            let after = row(&s);
            let c = after
                .cells
                .iter()
                .find(|c| c.column_id == "C_EDIT_USERS")
                .unwrap();
            assert_eq!(c.text, text);
            assert_eq!(matches!(&c.content, CellContent::People(_)), !flag);
            assert_eq!(s.slack_lists.fixture.as_ref().unwrap().records, records);
        }
    }
}
