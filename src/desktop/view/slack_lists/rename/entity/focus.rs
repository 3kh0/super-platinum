//! A bounded pre-mutation DOM handshake; no fabricated cell owner.
use super::*;
use std::time::Duration;
pub(super) fn remove(mut state: Signal<ShellState>, stamp: draft::Stamp, id: String) {
    spawn(async move {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
        let proof = serde_json::json!({"serial":stamp.serial.to_string(),"visit":stamp.visit.to_string(),
            "selection":stamp.selection.to_string(),"choice":stamp.choice.revision.to_string(),"epoch":stamp.epoch.to_string(),"id":id});
        let mut eval = dioxus::document::eval(&format!(
            "{};\nawait entityDefaultRemoveFocus({proof});",
            include_str!("focus.js")
        ));
        if !matches!(
            tokio::time::timeout_at(deadline, eval.recv::<bool>()).await,
            Ok(Ok(true))
        ) || tokio::time::Instant::now() >= deadline
        {
            let _ = eval.send(serde_json::Value::Null);
            return;
        }
        let changed = settings::edit_shell(&state.peek(), stamp, Edit::Remove(id));
        let epoch = changed
            .then(|| draft::presentation(&state.peek()).map(|p| p.stamp.epoch.to_string()))
            .flatten();
        if changed {
            state.write();
        }
        let _ = eval.send(epoch);
    });
}
