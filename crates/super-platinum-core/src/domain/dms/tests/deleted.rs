use super::*;
use crate::{config::WorkspaceSession, state::Workspace};

#[test]
fn selected_deleted_preview_evidence_outlives_old_page_but_not_new_body_or_owner() {
    let mut c = CoreAppState::new(Default::default());
    c.account_epoch = 3;
    c.active_team = Some("T".into());
    c.workspaces.insert(
        "T".into(),
        Workspace::from_session(&WorkspaceSession {
            team_id: "T".into(),
            user_id: "U1".into(),
            name: "Fixture".into(),
            url: "https://fixture.invalid".into(),
            token: "synthetic".into(),
            enterprise_id: None,
        }),
    );
    c.dms = published();
    let selected = c.dms.entries.iter().find(|e| e.id == "D1").unwrap().clone();
    assert!(!c.dm_preview_was_deleted(&selected));
    let before = serde_json::to_value(&c.dms.entries).unwrap();
    c.workspaces
        .get_mut("T")
        .unwrap()
        .messages
        .entry("D1".into())
        .or_default()
        .record_remote_deletion("2.1");
    assert!(c.dm_preview_was_deleted(&selected));
    assert_eq!(
        serde_json::to_value(&c.dms.entries).unwrap(),
        before,
        "display query never mutates data"
    );
    let peer = c.dms.entries.iter().find(|e| e.id == "D2").unwrap();
    assert!(!c.dm_preview_was_deleted(peer));
    c.dms.begin_preview_read(3, "T", 2);
    c.dms.publish_preview_page(
        3,
        "T",
        2,
        false,
        vec![
            entry("D1", "2.1", "Late old body"),
            entry("D2", "3.1", "Independent peer"),
        ],
    );
    assert!(c.dm_preview_was_deleted(c.dms.entries.iter().find(|e| e.id == "D1").unwrap()));
    c.dms.begin_preview_read(3, "T", 3);
    c.dms.publish_preview_page(
        3,
        "T",
        3,
        false,
        vec![
            entry("D1", "4.1", "New current body"),
            entry("D2", "3.1", "Independent peer"),
        ],
    );
    assert!(!c.dm_preview_was_deleted(c.dms.entries.iter().find(|e| e.id == "D1").unwrap()));
    assert!(
        !c.dm_preview_was_deleted(&selected),
        "old DTO cannot select the current row body"
    );
    c.dms.begin_preview_read(3, "T", 4);
    c.dms
        .publish_preview_page(3, "T", 4, false, vec![selected.clone()]);
    assert!(c.dm_preview_was_deleted(&selected));
    c.account_epoch += 1;
    assert!(
        !c.dm_preview_was_deleted(&selected),
        "retired publication is not current evidence"
    );
}
