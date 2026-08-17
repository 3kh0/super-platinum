use super::super::*;

pub(super) fn msg(ts: &str, text: &str) -> SlackMessage {
    SlackMessage {
        ts: Some(ts.to_owned()),
        text: Some(text.to_owned()),
        ..Default::default()
    }
}
