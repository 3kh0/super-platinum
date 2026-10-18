use super::*;
use serde::Deserialize;
use std::time::{Duration, Instant};

#[derive(Deserialize)]
struct Snapshot {
    token: String,
    owner: String,
    blocks: Vec<serde_json::Value>,
    post_at: i64,
}
pub(super) async fn read(
    root: &str,
    editor: &str,
    date: &str,
    owner: &str,
    as_draft: bool,
) -> Result<(Vec<serde_json::Value>, i64), String> {
    let token = api::ScheduleMessage::new(String::new(), None, vec![], 0).client_msg_id;
    let config = serde_json::json!({"root":root,"editor":editor,"date":date,"owner":owner,"token":token,"as_draft":as_draft});
    let started = Instant::now();
    let mut eval = document::eval(&format!(
        "{}\nawait captureScheduledForward({config},dioxus);",
        include_str!("capture.js")
    ));
    let result = tokio::time::timeout(Duration::from_secs(2), eval.recv::<Snapshot>()).await;
    let result = match result {
        Ok(Ok(snapshot)) if admits(&snapshot, &token, owner, started.elapsed()) => {
            Ok((snapshot.blocks, snapshot.post_at))
        }
        _ => Err(
            "The scheduling editor could not be captured. Your input is still here; try again."
                .into(),
        ),
    };
    let _ = eval.send(token);
    result
}

fn admits(snapshot: &Snapshot, token: &str, owner: &str, elapsed: Duration) -> bool {
    elapsed < Duration::from_secs(2) && snapshot.token == token && snapshot.owner == owner
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn post_receive_deadline_and_exact_visit_are_both_required() {
        let snapshot = Snapshot {
            token: "request".into(),
            owner: "record:4:visit-2".into(),
            blocks: vec![],
            post_at: 42,
        };
        assert!(admits(
            &snapshot,
            "request",
            "record:4:visit-2",
            Duration::from_millis(1999)
        ));
        assert!(!admits(
            &snapshot,
            "request",
            "record:4:visit-2",
            Duration::from_secs(2)
        ));
        assert!(!admits(
            &snapshot,
            "request",
            "record:4:visit-2",
            Duration::from_secs(5)
        ));
        assert!(!admits(
            &snapshot,
            "new-request",
            "record:4:visit-2",
            Duration::ZERO
        ));
        assert!(!admits(
            &snapshot,
            "request",
            "record:4:visit-3",
            Duration::ZERO
        ));
    }
}
