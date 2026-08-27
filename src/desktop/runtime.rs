use dioxus::prelude::{ReadableExt, Signal, WritableExt};

use crate::state::ShellState;

/// How often the tick checks for media that nothing else is going to fetch.
///
/// Sources registered during render (hover cards, activity rows, thread lists)
/// have no Slack call behind them, so without this sweep their images would stay
/// placeholders until the next unrelated refresh. The first tick sweeps too, so
/// cached avatars paint from disk without waiting on any Slack call.
const MEDIA_SWEEP_TICKS: u32 = 5;

pub async fn ticks(mut state: Signal<ShellState>) {
    let mut interval = tokio::time::interval(std::time::Duration::from_millis(200));
    let mut tick: u32 = 0;
    loop {
        interval.tick().await;
        tick = tick.wrapping_add(1);
        let now = std::time::Instant::now();
        let mut shell = state.write();
        // Both of these are time-driven rather than event-driven, and neither
        // earns a timer of its own.
        let mut probe_connection = false;
        if !crate::fixture::is_fixture() {
            shell.expire_toast(now);
            probe_connection = crate::connection::evaluate(&mut shell, now);
        }
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
        // Bumping the generation is what re-stamps painted `src` attributes, so
        // images swap from their placeholder as soon as any bytes land — one
        // slow host cannot hold back the whole batch.
        if shell.media.take_dirty() {
            shell.media_epoch = shell.media_epoch.wrapping_add(1);
        }
        let sweep =
            tick % MEDIA_SWEEP_TICKS == 1 && shell.media.has_pending() && !shell.media.is_loading();
        let transport = sweep.then(|| shell.core.transport.clone()).flatten();
        drop(shell);
        if probe_connection {
            dioxus::prelude::spawn(crate::connection::probe(state));
        }
        if let Some(transport) = transport {
            let media = state.read().media.clone();
            dioxus::prelude::spawn(async move { media.load_pending(transport).await });
        }
    }
}
