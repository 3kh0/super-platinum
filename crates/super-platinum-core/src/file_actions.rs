//! File action policy and reconciliation shared by local writes and realtime.
use crate::{
    CoreAppState,
    slack::{
        events::RawEvent,
        models::{File, Message},
    },
};

pub(crate) mod display_metadata;

/// The small, non-secret subset a rendered attachment needs for file actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileIdentity {
    pub id: String,
    pub permalink: Option<String>,
    pub uploader: Option<String>,
    pub renameable: bool,
}
pub fn identity(file: &File) -> Option<FileIdentity> {
    if deleted(file) {
        return None;
    }
    Some(FileIdentity {
        id: file.id.as_ref().filter(|id| !id.is_empty())?.clone(),
        permalink: permalink(file),
        uploader: crate::state::file_uploader_id(file).map(str::to_owned),
        renameable: renameable(file),
    })
}

pub fn permalink(file: &File) -> Option<String> {
    let url = url::Url::parse(file.extra.get("permalink")?.as_str()?).ok()?;
    (url.scheme() == "https"
        && url
            .host_str()
            .is_some_and(|h| h == "slack.com" || h.ends_with(".slack.com"))
        && url.username().is_empty()
        && url.password().is_none()
        && url.port_or_known_default() == Some(443))
    .then(|| url.to_string())
}

pub fn deleted(file: &File) -> bool {
    file.extra.get("mode").and_then(|v| v.as_str()) == Some("tombstone")
        || file.extra.get("is_tombstoned").and_then(|v| v.as_bool()) == Some(true)
}

pub fn can_delete(file: &File, user: &str) -> bool {
    !user.is_empty()
        && !deleted(file)
        && file.id.as_ref().is_some_and(|id| !id.is_empty())
        && crate::state::file_uploader_id(file) == Some(user)
}

pub fn deleted_event(raw: &RawEvent) -> Option<&str> {
    (raw.kind == "file_deleted").then_some(())?;
    raw.rest
        .get("file_id")
        .or_else(|| raw.rest.get("file"))?
        .as_str()
        .filter(|s| !s.is_empty())
}

/// Hosted uploads and snippets use files.edit; canvases/lists have separate editors.
pub fn renameable(file: &File) -> bool {
    !deleted(file)
        && file.is_external != Some(true)
        && !matches!(
            file.filetype.as_deref(),
            Some("quip" | "canvas" | "list" | "lists")
        )
        && !matches!(
            file.extra.get("mode").and_then(|v| v.as_str()),
            Some("external")
        )
}

pub fn rename_title(value: &str) -> Result<&str, &'static str> {
    let title = value.trim();
    if title.is_empty() {
        Err("Enter a name for this file.")
    } else if title.chars().any(char::is_control) {
        Err("Use a single line without control characters.")
    } else {
        Ok(title)
    }
}

pub(crate) fn visit_message(message: &mut Message, update: &mut impl FnMut(&mut File)) {
    for file in message
        .files
        .iter_mut()
        .chain(message.attachments.iter_mut().flat_map(|a| &mut a.files))
    {
        update(file);
    }
}

pub(crate) fn tombstone(id: &str) -> File {
    File {
        id: Some(id.into()),
        title: Some("Deleted file".into()),
        extra: [("mode".into(), "tombstone".into())].into(),
        ..Default::default()
    }
}

impl CoreAppState {
    /// Workspace tombstones outlive Later's transient account/workspace scope.
    /// Mask history and pending overlays, including the first visit to Later.
    pub fn hydrate_later_message(
        &mut self,
        team: &str,
        key: crate::later::SavedKey,
        message: Message,
        requested_revision: u64,
    ) {
        if !self.later.matches_scope(self.account_epoch, team) {
            return;
        }
        self.later
            .hydrate_message_since(key.clone(), message, requested_revision);
        let mut deleted = Vec::new();
        if let Some(workspace) = self.workspaces.get(team)
            && let Some(message) = self.later.hydrated.get_mut(&key)
        {
            visit_message(message, &mut |file| {
                if let Some(id) = file.id.as_deref()
                    && workspace.deleted_files.contains(id)
                {
                    deleted.push(id.to_owned());
                    *file = tombstone(id);
                }
            });
        }
        for id in deleted {
            self.later.record_file_deletion(&id);
        }
    }

    pub fn remove_file(&mut self, team: &str, id: &str) {
        if id.is_empty() {
            return;
        }
        if let Some(ws) = self.workspaces.get_mut(team) {
            ws.deleted_files.insert(id.into());
        }
        self.visit_files(team, |file| {
            if file.id.as_deref() == Some(id) {
                *file = tombstone(id);
            }
        });
        if self.later.matches_scope(self.account_epoch, team) {
            self.later.record_file_deletion(id);
        }
        for files in [&mut self.files, &mut self.channel_files] {
            if files.matches_scope(self.account_epoch, team) {
                files.remove(id);
            }
        }
    }

    /// Apply verified display/download names without replacing media URLs or shares.
    pub fn rename_file(&mut self, team: &str, id: &str, title: &str, download_name: Option<&str>) {
        if id.is_empty()
            || title.is_empty()
            || self
                .workspaces
                .get(team)
                .is_some_and(|ws| ws.deleted_files.contains(id))
        {
            return;
        }
        self.visit_files(team, |file| {
            if file.id.as_deref() == Some(id) && !deleted(file) {
                file.title = Some(title.into());
                if let Some(name) = download_name {
                    file.name = Some(name.into());
                }
            }
        });
        if self.later.matches_scope(self.account_epoch, team) {
            self.later.record_file_rename(id, title, download_name);
        }
        // A search started before this write must not paint an older title over it.
        for files in [&mut self.files, &mut self.channel_files] {
            if files.matches_scope(self.account_epoch, team) && files.loading {
                files.generation = files.generation.wrapping_add(1);
                files.loading = false;
            }
        }
    }

    fn visit_files(&mut self, team: &str, mut update: impl FnMut(&mut File)) {
        if let Some(ws) = self.workspaces.get_mut(team) {
            for bag in ws.messages.values_mut() {
                for message in &mut bag.messages {
                    visit_message(message, &mut update);
                }
            }
            self.cache_dirty
                .insert(team.into(), std::time::Instant::now());
        }
        for ((t, _, _), bag) in &mut self.threads {
            if t == team {
                for message in &mut bag.messages {
                    visit_message(message, &mut update);
                }
            }
        }
        for files in [&mut self.files, &mut self.channel_files] {
            if files.matches_scope(self.account_epoch, team) {
                for file in &mut files.items {
                    update(file);
                }
            }
        }
        if let Some(search) = &mut self.search {
            if search.team == team {
                for hit in &mut search.hits {
                    visit_message(&mut hit.message, &mut update);
                }
            }
        }
        if self.pins.matches_scope(self.account_epoch, team) {
            for item in self.pins.items.values_mut() {
                if let Some(message) = &mut item.message {
                    visit_message(message, &mut update);
                }
            }
        }
        if self.threads_view.matches_scope(self.account_epoch, team) {
            for item in &mut self.threads_view.items {
                visit_message(&mut item.root_msg, &mut update);
                for message in item
                    .latest_replies
                    .iter_mut()
                    .chain(&mut item.unread_replies)
                {
                    visit_message(message, &mut update);
                }
            }
        }
        if self.activity.matches_scope(self.account_epoch, team) {
            for message in self.activity.hydrated.values_mut() {
                visit_message(message, &mut update);
            }
        }
        if self.later.matches_scope(self.account_epoch, team) {
            self.later.visit_files(&mut update);
        }
        if self.active_team.as_deref() == Some(team) {
            for dm in &mut self.dms.entries {
                if let Some(message) = &mut dm.message {
                    visit_message(message, &mut update);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn file() -> File {
        serde_json::from_value(json!({"id":"F1","user":"U1","title":"Release notes","permalink":"https://team.slack.com/files/U1/F1","url_private":"https://files.slack.com/private","thumb_64":"https://files.slack.com/thumb"})).unwrap()
    }
    #[test]
    fn later_hydration_keeps_workspace_tombstones_across_scope_changes() {
        let mut core = CoreAppState::new(Default::default());
        core.workspaces.insert(
            "T1".into(),
            crate::state::Workspace::from_session(&crate::config::WorkspaceSession {
                team_id: "T1".into(),
                enterprise_id: None,
                user_id: "U1".into(),
                name: "Team".into(),
                url: "https://example.slack.com".into(),
                token: String::new(),
            }),
        );
        core.remove_file("T1", "F1");
        let key = crate::later::SavedKey::message("C1", "1.0");
        let message = || Message {
            ts: Some("1.0".into()),
            text: Some("Keep text".into()),
            files: vec![file()],
            attachments: vec![crate::slack::models::Attachment {
                files: vec![file()],
                ..Default::default()
            }],
            ..Default::default()
        };
        for _ in 0..2 {
            core.later.ensure_scope(0, "T1");
            core.later.items.insert(key.clone(), Default::default());
            let revision = core.later.begin_hydration();
            core.later.update_message("C1", &message());
            core.hydrate_later_message("T1", key.clone(), message(), revision);
            let saved = &core.later.hydrated[&key];
            assert!(deleted(&saved.files[0]));
            assert!(deleted(&saved.attachments[0].files[0]));
            assert_eq!(saved.text.as_deref(), Some("Keep text"));
            core.later.update_message("C1", &message());
            assert!(deleted(&core.later.hydrated[&key].files[0]));
            core.later.ensure_scope(0, "T2");
            core.hydrate_later_message("T1", key.clone(), message(), revision);
            assert!(core.later.hydrated.is_empty());
        }
    }
    #[test]
    fn permissions_links_and_deleted_shapes_are_conservative() {
        let mut file = file();
        let metadata = identity(&file).unwrap();
        assert_eq!(metadata.id, "F1");
        assert_eq!(metadata.uploader.as_deref(), Some("U1"));
        assert_eq!(
            metadata.permalink.as_deref(),
            Some("https://team.slack.com/files/U1/F1")
        );
        assert!(can_delete(&file, "U1"));
        assert!(!can_delete(&file, "U2"));
        assert!(permalink(&file).is_some());
        for link in [
            "javascript:alert(1)",
            "https://slack.com.evil.test/file",
            "https://user:password@team.slack.com/file",
            "http://team.slack.com/file",
        ] {
            file.extra.insert("permalink".into(), link.into());
            assert!(permalink(&file).is_none());
            assert!(identity(&file).unwrap().permalink.is_none());
        }
        file.extra.insert("mode".into(), "tombstone".into());
        assert!(!can_delete(&file, "U1"));
        assert!(identity(&file).is_none());
        for key in ["file_id", "file"] {
            let raw = serde_json::from_value(json!({"type":"file_deleted",key:"F1"})).unwrap();
            assert_eq!(deleted_event(&raw), Some("F1"));
        }
        let raw = serde_json::from_value(json!({"type":"file_unshared","file_id":"F1"})).unwrap();
        assert_eq!(deleted_event(&raw), None);
    }
    #[test]
    fn title_validation_and_file_kinds_preserve_unicode_and_separate_editors() {
        assert_eq!(
            rename_title("  Notes · 日本語 🦊  "),
            Ok("Notes · 日本語 🦊")
        );
        for bad in [" ", "one\ntwo", "null\0byte", "tab\tname"] {
            assert!(rename_title(bad).is_err());
        }
        let mut file = file();
        assert!(renameable(&file));
        file.is_external = Some(true);
        assert!(!renameable(&file));
        file.is_external = Some(false);
        for kind in ["canvas", "quip", "list", "lists"] {
            file.filetype = Some(kind.into());
            assert!(!renameable(&file));
        }
    }
    #[test]
    fn later_reconciles_pending_and_visible_files_without_overriding_fresh_history() {
        let mut core = CoreAppState::new(Default::default());
        core.later.ensure_scope(0, "T1");
        let key = crate::later::SavedKey::message("C1", "1.0");
        let original = Message {
            ts: Some("1.0".into()),
            user: Some("U1".into()),
            text: Some("Preserve message text".into()),
            files: vec![file()],
            attachments: vec![crate::slack::models::Attachment {
                text: Some("Preserve attachment text".into()),
                files: vec![file()],
                ..Default::default()
            }],
            ..Default::default()
        };
        core.later.hydrate_message(key.clone(), original.clone());
        let old_request = core.later.hydration_revision();
        core.later.update_message("C1", &original);
        core.rename_file("T2", "F1", "Other workspace", None);
        assert_eq!(
            core.later.hydrated[&key].files[0].title,
            original.files[0].title
        );
        core.rename_file("T1", "F1", "Renamed · 日本語", Some("renamed.txt"));
        assert_eq!(
            core.later.hydrated[&key].files[0].title.as_deref(),
            Some("Renamed · 日本語")
        );
        core.later
            .hydrate_message_since(key.clone(), original.clone(), old_request);
        let renamed = &core.later.hydrated[&key];
        assert_eq!(renamed.text, original.text);
        assert_eq!(renamed.user, original.user);
        assert_eq!(renamed.attachments[0].text, original.attachments[0].text);
        for file in [&renamed.files[0], &renamed.attachments[0].files[0]] {
            assert_eq!(file.title.as_deref(), Some("Renamed · 日本語"));
            assert_eq!(file.name.as_deref(), Some("renamed.txt"));
            assert!(file.full_url().is_some());
        }
        let fresh_request = core.later.hydration_revision();
        core.later.update_message(
            "C1",
            &Message {
                ts: original.ts.clone(),
                text: Some("Concurrent text edit".into()),
                ..Default::default()
            },
        );
        let mut newer = original.clone();
        visit_message(&mut newer, &mut |file| {
            file.title = Some("Changed in another client".into());
            file.name = Some("another-client.txt".into());
        });
        core.later
            .hydrate_message_since(key.clone(), newer.clone(), fresh_request);
        let fresh = &core.later.hydrated[&key];
        assert_eq!(fresh.text.as_deref(), Some("Concurrent text edit"));
        for file in [&fresh.files[0], &fresh.attachments[0].files[0]] {
            assert_eq!(file.title.as_deref(), Some("Changed in another client"));
            assert_eq!(file.name.as_deref(), Some("another-client.txt"));
        }
        // A later authoritative file-bearing message frame also wins over
        // an earlier rename when an even older history request returns.
        core.later.update_message("C1", &newer);
        core.later
            .hydrate_message_since(key.clone(), original.clone(), old_request);
        for file in [
            &core.later.hydrated[&key].files[0],
            &core.later.hydrated[&key].attachments[0].files[0],
        ] {
            assert_eq!(file.title.as_deref(), Some("Changed in another client"));
        }
        core.remove_file("T1", "F1");
        core.later.update_message("C1", &original);
        assert!(deleted(&core.later.hydrated[&key].files[0]));
        let request = core.later.hydration_revision();
        core.later
            .hydrate_message_since(key.clone(), original, request);
        let removed = &core.later.hydrated[&key];
        assert_eq!(removed.text.as_deref(), Some("Preserve message text"));
        assert!(deleted(&removed.files[0]));
        assert!(deleted(&removed.attachments[0].files[0]));
        assert!(removed.files[0].full_url().is_none());
    }
    #[test]
    fn deletion_reconciles_every_cached_message_copy_without_deleting_text() {
        let mut core = CoreAppState::new(Default::default());
        core.active_team = Some("T1".into());
        for team in ["T1", "T2"] {
            let session = crate::config::WorkspaceSession {
                team_id: team.into(),
                enterprise_id: None,
                user_id: "U1".into(),
                name: team.into(),
                url: "https://example.slack.com".into(),
                token: String::new(),
            };
            let mut ws = crate::state::Workspace::from_session(&session);
            let message = Message {
                ts: Some("1.0".into()),
                text: Some("Keep this note".into()),
                files: vec![file()],
                attachments: vec![crate::slack::models::Attachment {
                    files: vec![file()],
                    ..Default::default()
                }],
                ..Default::default()
            };
            let bag = crate::state::ChannelMessages {
                messages: vec![message],
                ..Default::default()
            };
            core.threads
                .insert((team.into(), "C1".into(), "1.0".into()), bag.clone());
            ws.messages.insert("C1".into(), bag);
            core.workspaces.insert(team.into(), ws);
        }
        core.files.ensure_scope(0, "T1");
        core.channel_files.ensure_scope(0, "T1");
        for store in [&mut core.files, &mut core.channel_files] {
            store.items = vec![file()];
            store.total = 1;
            store.page = 1;
        }
        let old_search = core.files.begin(Default::default(), 1);
        core.rename_file("T1", "F1", "Renamed · 日本語", Some("Renamed.txt"));
        for msg in [
            &core.workspaces["T1"].messages["C1"].messages[0],
            &core.threads[&("T1".into(), "C1".into(), "1.0".into())].messages[0],
        ] {
            for renamed in [&msg.files[0], &msg.attachments[0].files[0]] {
                let mut expected = file();
                expected.title = Some("Renamed · 日本語".into());
                expected.name = Some("Renamed.txt".into());
                assert_eq!(
                    serde_json::to_value(renamed).unwrap(),
                    serde_json::to_value(expected).unwrap()
                );
            }
        }
        assert_eq!(
            core.channel_files.items[0].title.as_deref(),
            Some("Renamed · 日本語")
        );
        assert_eq!(
            core.workspaces["T2"].messages["C1"].messages[0].files[0]
                .title
                .as_deref(),
            Some("Release notes")
        );
        assert!(!core.files.apply(
            old_search,
            crate::files::FilesPage {
                items: vec![file()],
                ..Default::default()
            }
        ));
        assert!(!core.files.loading);
        core.remove_file("T1", "F1");
        core.remove_file("T1", "F1");
        core.rename_file("T1", "F1", "Do not resurrect", Some("resurrect.txt"));
        for msg in [
            &core.workspaces["T1"].messages["C1"].messages[0],
            &core.threads[&("T1".into(), "C1".into(), "1.0".into())].messages[0],
        ] {
            assert_eq!(msg.text.as_deref(), Some("Keep this note"));
            assert!(deleted(&msg.files[0]));
            assert!(msg.files[0].full_url().is_none());
            assert!(msg.files[0].preview().is_none());
            assert!(deleted(&msg.attachments[0].files[0]));
        }
        assert!(!deleted(
            &core.workspaces["T2"].messages["C1"].messages[0].files[0]
        ));
        assert!(core.files.items.is_empty() && core.channel_files.items.is_empty());
        assert_eq!(core.files.total, 0);
        let generation = core.files.begin(Default::default(), 1);
        assert!(core.files.apply(
            generation,
            crate::files::FilesPage {
                items: vec![file()],
                ..Default::default()
            }
        ));
        assert!(core.files.items.is_empty());
        assert_eq!(core.files.total, 0);
        core.files.ensure_scope(1, "T1");
        assert!(core.files.deleted.is_empty());
    }
}
