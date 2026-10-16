//! Human-facing copy only. This projection never authorizes another Share.
use super_platinum_core::{
    file_sharing::{Receipt, State},
    slack::api::file_share::{FileShareDisposition, FileSharePolicy},
};

#[derive(Clone, PartialEq)]
pub(super) struct Status {
    pub title: String,
    pub detail: &'static str,
    pub custom: Option<String>,
    pub result_not_saved: bool,
}

pub(super) fn receipt(receipt: &Receipt, retained: Option<State>) -> Status {
    project(receipt.state(), retained, &receipt.destination_name)
}
fn project(saved: State, retained: Option<State>, destination: &str) -> Status {
    let result_not_saved = retained.is_some();
    let mut status = describe(retained.unwrap_or(saved), destination);
    status.result_not_saved = result_not_saved;
    status
}

fn describe(state: State, destination: &str) -> Status {
    let (title, detail, custom) = match state {
        State::Server(FileShareDisposition::Accepted { .. }) => (
            format!("Shared to {destination}"),
            "Slack accepted this share.",
            None,
        ),
        State::NotSent(_) => (
            "File not shared".into(),
            "Nothing was sent. Your message and destination are retained.",
            None,
        ),
        State::Server(FileShareDisposition::DlpWarning { message }) => (
            "Sharing needs review".into(),
            "Review this file in Slack. Your message and destination are saved.",
            message,
        ),
        State::Server(FileShareDisposition::DlpProcessing) => (
            "Slack is checking this file".into(),
            "Sharing is not confirmed. Check in Slack before sharing again.",
            None,
        ),
        State::Server(FileShareDisposition::Policy(policy)) => (
            "Slack could not share this file".into(),
            match policy {
                FileSharePolicy::FileType => {
                    "This file type cannot be shared to that conversation. Review the file in Slack."
                }
                FileSharePolicy::PostingDenied | FileSharePolicy::RestrictedAction => {
                    "Sharing to this conversation is restricted. Review the file in Slack."
                }
                FileSharePolicy::FileUnavailable => {
                    "This file is unavailable. Review its status in Slack."
                }
                FileSharePolicy::DestinationUnavailable => {
                    "This conversation is unavailable. Review its status in Slack."
                }
                FileSharePolicy::MessageLimit | FileSharePolicy::ShareLimit => {
                    "Slack has reached a sharing limit. Review the file in Slack."
                }
                FileSharePolicy::SlackConnectFileType
                | FileSharePolicy::SlackConnectLink
                | FileSharePolicy::SlackConnectUpload
                | FileSharePolicy::ExternalGuest => {
                    "Slack restricted this share. Review the file and conversation in Slack."
                }
            },
            None,
        ),
        State::RateLimited { .. } => (
            "Slack asked us to wait".into(),
            "Sharing is not confirmed. Your message is saved; check in Slack before sharing again.",
            None,
        ),
        State::Unconfirmed | State::Server(FileShareDisposition::Uncertain(_)) => (
            "Sharing is not confirmed".into(),
            "Your message and destination are saved. Check in Slack before sharing again.",
            None,
        ),
    };
    Status {
        title,
        detail,
        custom,
        result_not_saved: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super_platinum_core::{
        file_sharing::NotSent, slack::api::file_share::FileShareUncertainty,
    };

    #[test]
    fn accepted_copy_uses_destination_without_claiming_observed_local_message() {
        let status = describe(
            State::Server(FileShareDisposition::Accepted { message_ts: None }),
            "design 日本語",
        );
        assert_eq!(status.title, "Shared to design 日本語");
        assert_eq!(status.detail, "Slack accepted this share.");
        assert!(status.custom.is_none());
    }
    #[test]
    fn genuine_unsent_is_distinct_from_unknown_and_rate_limit() {
        assert_eq!(
            describe(State::NotSent(NotSent::Offline), "channel").title,
            "File not shared"
        );
        for state in [
            State::Unconfirmed,
            State::RateLimited {
                retry_after_secs: Some(30),
            },
            State::Server(FileShareDisposition::Uncertain(
                FileShareUncertainty::ResponseLimit,
            )),
        ] {
            let status = describe(state, "channel");
            assert_ne!(status.title, "File not shared");
            assert!(status.detail.contains("before sharing again"));
        }
    }
    #[test]
    fn dlp_custom_copy_stays_exact_text_and_processing_stays_unconfirmed() {
        let message = "Review <script>private</script> & 日本語".to_owned();
        let status = describe(
            State::Server(FileShareDisposition::DlpWarning {
                message: Some(message.clone()),
            }),
            "channel",
        );
        assert_eq!(status.custom, Some(message));
        let status = describe(
            State::Server(FileShareDisposition::DlpProcessing),
            "channel",
        );
        assert!(status.detail.contains("not confirmed"));
    }
    #[test]
    fn retained_outcome_overrides_uncertain_record_without_claiming_saved_result() {
        let status = project(
            State::Unconfirmed,
            Some(State::Server(FileShareDisposition::Accepted {
                message_ts: None,
            })),
            "design",
        );
        assert_eq!(status.title, "Shared to design");
        assert!(status.result_not_saved);
        let saved = project(
            State::Server(FileShareDisposition::Accepted { message_ts: None }),
            None,
            "design",
        );
        assert_eq!(saved.title, status.title);
        assert!(!saved.result_not_saved);
        let unknown = project(State::Unconfirmed, None, "design");
        assert_eq!(unknown.title, "Sharing is not confirmed");
        assert!(!unknown.result_not_saved);
    }
    #[test]
    fn policy_copy_never_claims_safe_retry_or_exposes_wire_codes() {
        for policy in [
            FileSharePolicy::FileType,
            FileSharePolicy::SlackConnectFileType,
            FileSharePolicy::SlackConnectLink,
            FileSharePolicy::SlackConnectUpload,
            FileSharePolicy::PostingDenied,
            FileSharePolicy::RestrictedAction,
            FileSharePolicy::ExternalGuest,
            FileSharePolicy::MessageLimit,
            FileSharePolicy::ShareLimit,
            FileSharePolicy::FileUnavailable,
            FileSharePolicy::DestinationUnavailable,
        ] {
            let status = describe(
                State::Server(FileShareDisposition::Policy(policy)),
                "channel",
            );
            assert_eq!(status.title, "Slack could not share this file");
            assert!(!status.detail.contains("retry"));
            assert!(!status.detail.contains('_'));
        }
    }
}
