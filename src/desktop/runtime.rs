use dioxus::prelude::{Signal, WritableExt};

use crate::state::ShellState;

pub async fn ticks(mut state: Signal<ShellState>) {
    let mut interval = tokio::time::interval(std::time::Duration::from_millis(200));
    loop {
        interval.tick().await;
        let now = std::time::Instant::now();
        let mut shell = state.write();
        let typing_changed = shell
            .core
            .workspaces
            .values_mut()
            .any(|workspace| workspace.prune_typing(now, std::time::Duration::from_secs(4)));
        let before = shell.message_arrivals.len();
        shell.message_arrivals.retain(|_, started| {
            now.duration_since(*started) < std::time::Duration::from_millis(450)
        });
        let uploading = shell
            .core
            .composer_attachments
            .iter()
            .chain(shell.core.thread_composer_attachments.iter())
            .chain(
                shell
                    .core
                    .pending_file_messages
                    .iter()
                    .flat_map(|pending| pending.attachments.iter()),
            )
            .any(|attachment| attachment.uploading);
        // Writing the signal re-renders attachment progress rings from AtomicU64 values.
        if typing_changed || shell.message_arrivals.len() != before || uploading {
            if typing_changed {
                shell.refresh_from_core();
            }
            shell.upload_ui_epoch = shell.upload_ui_epoch.wrapping_add(1);
        }
    }
}
