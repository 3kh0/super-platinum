use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
static LEASE: AtomicU64 = AtomicU64::new(1);
pub(in crate::canvas) fn use_focus(
    state: Signal<ShellState>,
    model: Signal<State>,
    opened: Signal<bool>,
) {
    let lease = use_hook(|| LEASE.fetch_add(1, Ordering::Relaxed).to_string());
    let effect_lease = lease.clone();
    use_effect(move || {
        let open = *opened.read();
        let _revision = model.read().serial;
        let _loading = model.read().loading;
        let visit = state.read().shortcut_generation.to_string();
        document::eval(&format!(
            "{};window.superPlatinumCanvasCommentFocus.sync({},{},{});",
            include_str!("focus.js"),
            serde_json::json!(effect_lease),
            serde_json::json!(visit),
            open
        ));
    });
    use_drop(move || {
        document::eval(&format!(
            "window.superPlatinumCanvasCommentFocus?.release({});",
            serde_json::json!(lease)
        ));
    });
}
