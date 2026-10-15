//! Current exact thin-capture expression; compiled alongside the packed arm.
use super::{CustomRecord, Emoji, HashMap};
pub(super) fn capture(workspace_custom_emoji: &HashMap<String, Emoji>) -> Vec<CustomRecord> {
    workspace_custom_emoji
        .values()
        .map(CustomRecord::from)
        .collect()
}
