//! Static disconnected pin-file scene, using the production reconciliation path.
use super_platinum_core::{CoreAppState, pins::PinnedItem, slack::models::File};

pub(super) fn apply(core: &mut CoreAppState, team: &str, channel: &str) {
    for (index, (id, title, comment)) in [
        ("FPINRENAMED", "Original release notes", None),
        ("FPINDELETED", "Removed attachment", None),
        (
            "FPINCOMMENT",
            "Removed comment attachment",
            Some("Original note retained after deletion."),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let file: File = serde_json::from_value(serde_json::json!({
            "id":id,"user":"U1","title":title,"name":format!("{id}.txt"),
            "size":24,"pretty_type":"Plain text","filetype":"text","mimetype":"text/plain",
            "url_private":format!("https://files.slack.com/fixture/{id}")
        }))
        .expect("pin file fixture");
        let item = PinnedItem {
            kind: if comment.is_some() {
                "file_comment"
            } else {
                "file"
            }
            .into(),
            channel: Some(channel.into()),
            file: Some(file),
            comment: comment.map(|text| serde_json::json!({"id":"FCPIN","comment":text})),
            created: Some(1792022400 - index as i64 * 86400),
            created_by: Some("U1".into()),
            ..Default::default()
        };
        core.pins.items.insert(item.key().unwrap(), item);
    }
    core.rename_file(
        team,
        "FPINRENAMED",
        "Renamed release notes.txt",
        Some("release-notes.txt"),
    );
    core.remove_file(team, "FPINDELETED");
    core.remove_file(team, "FPINCOMMENT");
}

#[cfg(test)]
mod tests {
    use crate::{
        media::MediaRegistry,
        state::{ChannelTab, ShellState},
    };
    use super_platinum_core::file_actions::{deleted, identity};

    #[test]
    fn normal_and_narrow_pin_files_fixtures_preserve_message_and_reconcile_file_rows() {
        let baseline = ShellState::fixture_variant(MediaRegistry::default(), "pins");
        let message = baseline.core.pins.ordered_items()[0];
        let message_key = message.key().unwrap();
        let expected_message = serde_json::to_value(message.message.as_ref().unwrap()).unwrap();
        for name in ["pins-files", "pins-files-narrow"] {
            let shell = ShellState::fixture_variant(MediaRegistry::default(), name);
            assert!(shell.core.session.is_none() && shell.core.transport.is_none());
            assert!(matches!(shell.channel_tab, ChannelTab::Pins));
            assert!(shell.core.pins.loaded && !shell.core.pins.loading);
            let rows = shell.core.pins.ordered_items();
            assert_eq!(
                rows.iter()
                    .map(|row| row.key().unwrap())
                    .collect::<Vec<_>>(),
                vec![
                    "file:FPINRENAMED".to_owned(),
                    "file:FPINDELETED".to_owned(),
                    "comment:FCPIN".to_owned(),
                    message_key.clone()
                ]
            );
            let renamed = rows[0].file.as_ref().unwrap();
            assert_eq!(renamed.title.as_deref(), Some("Renamed release notes.txt"));
            assert_eq!(renamed.name.as_deref(), Some("release-notes.txt"));
            assert_eq!(renamed.size, Some(24));
            assert!(!deleted(renamed) && identity(renamed).is_some());
            assert_eq!(
                renamed.url_private.as_deref(),
                Some("https://files.slack.com/fixture/FPINRENAMED")
            );
            for row in &rows[1..3] {
                let file = row.file.as_ref().unwrap();
                assert!(deleted(file) && identity(file).is_none());
                assert_eq!(super_platinum_core::state::file_title(file), "Deleted file");
                assert!(file.full_url().is_none());
                assert!(
                    shell.core.workspaces["T1"]
                        .deleted_files
                        .contains(file.id.as_ref().unwrap())
                );
            }
            assert_eq!(
                rows[2].comment.as_ref().unwrap()["comment"],
                "Original note retained after deletion."
            );
            assert_eq!(
                serde_json::to_value(rows[3].message.as_ref().unwrap()).unwrap(),
                expected_message
            );
        }
    }
}
