//! Recoverable local composers. Slack's server drafts remain a separate queue.
use crate::{ComposerState, domain::ComposerAttachment};
use std::collections::BTreeMap;
pub mod codec;
pub mod store;
pub mod sync;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct DraftKey {
    pub team: String,
    pub channel: String,
    pub thread: Option<String>,
}
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct LocalDraft {
    pub composer: ComposerState,
    pub attachments: Vec<ComposerAttachment>,
}
impl LocalDraft {
    pub fn empty(&self) -> bool {
        self.composer.text.is_empty()
            && self.attachments.is_empty()
            && !self.composer.reply_broadcast
    }
    fn matches(&self, composer: &ComposerState, files: &[ComposerAttachment]) -> bool {
        self.composer == *composer
            && self.attachments.len() == files.len()
            && self.attachments.iter().zip(files).all(|(a, b)| {
                a.id == b.id && a.path == b.path && a.name == b.name && a.bytes == b.bytes
            })
    }
}
#[derive(Debug, Default)]
pub struct DraftBook {
    pub items: BTreeMap<DraftKey, LocalDraft>,
    pub channel: Option<DraftKey>,
    pub thread: Option<DraftKey>,
    changed: BTreeMap<DraftKey, LocalDraft>,
}
impl DraftBook {
    pub fn restored(items: Vec<(DraftKey, LocalDraft)>) -> Self {
        Self {
            items: items.into_iter().collect(),
            ..Default::default()
        }
    }
    pub fn capture(
        &mut self,
        key: &DraftKey,
        composer: &ComposerState,
        files: &[ComposerAttachment],
    ) {
        if self
            .items
            .get(key)
            .is_some_and(|draft| draft.matches(composer, files))
            || (!self.items.contains_key(key)
                && composer.text.is_empty()
                && files.is_empty()
                && !composer.reply_broadcast)
        {
            return;
        }
        let draft = LocalDraft {
            composer: composer.clone(),
            attachments: files.to_vec(),
        };
        if draft.empty() {
            self.items.remove(key);
        } else {
            self.items.insert(key.clone(), draft.clone());
        }
        self.changed.insert(key.clone(), draft);
    }
    pub fn bind(
        &mut self,
        key: Option<DraftKey>,
        thread: bool,
        composer: &mut ComposerState,
        files: &mut Vec<ComposerAttachment>,
    ) {
        let previous = if thread { &self.thread } else { &self.channel };
        if *previous == key {
            return;
        }
        if let Some(previous) = previous.clone() {
            self.capture(&previous, composer, files);
        }
        let draft = key
            .as_ref()
            .and_then(|key| self.items.get(key))
            .cloned()
            .unwrap_or_default();
        *composer = draft.composer;
        *files = draft.attachments;
        if thread {
            self.thread = key;
        } else {
            self.channel = key;
        }
    }
    pub fn take_changes(&mut self) -> BTreeMap<DraftKey, LocalDraft> {
        std::mem::take(&mut self.changed)
    }
    /// An upload canceled after navigation belongs to its original composer.
    pub fn return_upload(&mut self, key: DraftKey, text: String, files: Vec<ComposerAttachment>) {
        self.return_upload_with_broadcast(key, text, files, false);
    }
    pub fn return_upload_with_broadcast(
        &mut self,
        key: DraftKey,
        text: String,
        files: Vec<ComposerAttachment>,
        reply_broadcast: bool,
    ) {
        let mut draft = self.items.get(&key).cloned().unwrap_or_default();
        if draft.composer.text.is_empty() && !draft.composer.reply_broadcast {
            draft.composer = ComposerState::with_text(text);
            draft.composer.reply_broadcast = key.thread.is_some() && reply_broadcast;
        }
        for file in files {
            if !draft.attachments.iter().any(|old| old.id == file.id) {
                draft.attachments.push(file);
            }
        }
        self.capture(&key, &draft.composer, &draft.attachments);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn key(channel: &str, thread: Option<&str>) -> DraftKey {
        DraftKey {
            team: "T1".into(),
            channel: channel.into(),
            thread: thread.map(str::to_owned),
        }
    }
    #[test]
    fn navigation_preserves_distinct_channel_and_thread_drafts() {
        let mut book = DraftBook::default();
        let mut composer = ComposerState::default();
        let mut files = vec![];
        book.bind(Some(key("C1", None)), false, &mut composer, &mut files);
        composer = ComposerState::with_text("*First 🚀*");
        book.bind(Some(key("C2", None)), false, &mut composer, &mut files);
        assert!(composer.text.is_empty());
        composer = ComposerState::with_text("Second");
        book.bind(Some(key("C1", None)), false, &mut composer, &mut files);
        assert_eq!(composer.text, "*First 🚀*");
        let mut reply = ComposerState::default();
        let mut reply_files = vec![];
        book.bind(
            Some(key("C1", Some("1.1"))),
            true,
            &mut reply,
            &mut reply_files,
        );
        reply = ComposerState::with_text("Reply one");
        book.bind(
            Some(key("C1", Some("2.2"))),
            true,
            &mut reply,
            &mut reply_files,
        );
        assert!(reply.text.is_empty());
        book.bind(None, true, &mut reply, &mut reply_files);
        book.bind(
            Some(key("C1", Some("1.1"))),
            true,
            &mut reply,
            &mut reply_files,
        );
        assert_eq!(reply.text, "Reply one");
        assert_eq!(composer.text, "*First 🚀*");
    }
    #[test]
    fn sent_or_cleared_drafts_do_not_resurrect() {
        let key = key("C1", None);
        let mut book = DraftBook::default();
        book.capture(&key, &ComposerState::with_text("send"), &[]);
        book.take_changes();
        book.capture(&key, &ComposerState::default(), &[]);
        assert!(book.take_changes()[&key].empty());
        assert!(!book.items.contains_key(&key));
        book.capture(&key, &ComposerState::default(), &[]);
        assert!(book.take_changes().is_empty());
    }
    #[test]
    fn failed_upload_restores_original_caption_without_overwriting_new_input() {
        let key = key("C1", Some("1.1"));
        let mut book = DraftBook::default();
        book.return_upload(key.clone(), "Original caption 🚀".into(), vec![]);
        assert_eq!(book.items[&key].composer.text, "Original caption 🚀");
        book.capture(&key, &ComposerState::with_text("New reply"), &[]);
        book.return_upload(key.clone(), "Old caption".into(), vec![]);
        assert_eq!(book.items[&key].composer.text, "New reply");
    }
    #[test]
    fn broadcast_draft_migrates_and_survives_flag_only_navigation_and_upload_restore() {
        let legacy: ComposerState = serde_json::from_value(
            serde_json::json!({"text":"Reply","selection":{"start":0,"end":0}}),
        )
        .unwrap();
        assert!(!legacy.reply_broadcast);
        let mut composer = ComposerState::default();
        composer.reply_broadcast = true;
        let restored: ComposerState =
            serde_json::from_value(serde_json::to_value(&composer).unwrap()).unwrap();
        assert_eq!(restored, composer);
        let key = key("C1", Some("1.1"));
        let mut book = DraftBook::default();
        book.capture(&key, &composer, &[]);
        assert!(!book.items[&key].empty());
        book.return_upload_with_broadcast(key.clone(), "Old caption".into(), vec![], false);
        assert!(book.items[&key].composer.reply_broadcast);
        assert!(book.items[&key].composer.text.is_empty());
        book.capture(&key, &ComposerState::default(), &[]);
        book.return_upload_with_broadcast(key.clone(), "Caption".into(), vec![], true);
        assert!(book.items[&key].composer.reply_broadcast);
        assert_eq!(book.items[&key].composer.text, "Caption");
    }
}
