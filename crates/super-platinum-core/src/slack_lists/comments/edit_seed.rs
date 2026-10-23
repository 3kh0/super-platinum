//! Source-derived mode planning and plain EDIT initialization only.
//! Observations must be acquired by the caller; no edit/send authority or IO.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
mod delta;
pub mod local_search;
pub mod mode;
pub mod plain;
pub mod prepare;
mod regex;
mod tokenizer;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SeedUnavailable {
    MalformedMessage,
    UnknownEmojiPreference,
    MissingUser(String),
    MissingChannel(String),
    MissingChannelToken(Value),
    MissingUsergroup(String),
    MissingEmoji(String),
    MissingBroadcast(String),
    MissingHighlights,
    MissingEmojiText(Value),
    MissingSlug(Value),
    UnsupportedToken(String),
    InvalidObservation,
    InvalidUnicode,
    Patterns,
    Limit,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct SeedObservations {
    pub emoji_as_text: Option<bool>,
    /// Already resolved labels including @/#, from the captured workspace.
    pub users: BTreeMap<String, String>,
    pub channels: BTreeMap<String, String>,
    pub usergroups: BTreeMap<String, String>,
    pub broadcasts: BTreeMap<String, String>,
    /// Exact colon token -> actual resolved standard/custom emoji observation.
    pub emoji: BTreeMap<String, EmojiObservation>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EmojiObservation {
    pub name: String,
    pub unicode: Option<String>,
    pub skin_tone: Option<u8>,
}
