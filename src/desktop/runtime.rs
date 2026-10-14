use dioxus::prelude::{ReadableExt, Signal, WritableExt};
use super_platinum_core::slack::transport::Health;

use crate::state::ShellState;

mod timing;

pub async fn ticks(mut state: Signal<ShellState>) {
    let mut runtime_probe = crate::performance::runtime_probe::heartbeat();
    let mut interval = timing::interval();
    let mut media_sweep = timing::MediaSweep::default();
    let mut draft_save_error = None;
    loop {
        interval.tick().await;
        if runtime_probe.is_some_and(|probe| !probe.beat()) {
            runtime_probe = None;
        }
        let now = std::time::Instant::now();
        // Bookkeeping (probe deadlines, route checks, etc.) must not repaint the
        // entire shell five times a second. Notify only for visible changes.
        // The timer owns private deadlines as well as UI state; a normal write
        // below explicitly notifies when any visible field actually changes.
        #[allow(deprecated)]
        let mut shell = state.write_silent();
        #[cfg(target_os = "macos")]
        crate::notification::attention::maintain(&shell.core);
        let mut connection = crate::connection::Next::Nothing;
        let mut changed = crate::app_modal::poll(&mut shell);
        changed |= crate::self_status::poll(&mut shell);
        changed |= crate::profile_edit::poll(&mut shell);
        changed |= crate::profile_title::poll(&mut shell);
        changed |= crate::channel_mute::poll(&mut shell);
        changed |= crate::notification_schedule::poll(&mut shell);
        changed |= crate::presentation_notifications::poll(&mut shell);
        changed |= crate::global_notifications::poll(&mut shell);
        changed |= crate::slack_lists::poll(&mut shell);
        changed |= crate::message_buttons::poll(&mut shell);
        changed |= crate::drafts::integration::poll(&mut shell);
        if !crate::fixture::is_fixture() {
            let failed_account = shell
                .core
                .active_account
                .as_ref()
                .filter(|account| super_platinum_core::drafts::store::save_failed(account))
                .cloned();
            if failed_account.is_some() && failed_account != draft_save_error {
                let retained_version = shell.managed_drafts.account == failed_account
                    && shell.managed_drafts.epoch == shell.core.account_epoch
                    && shell
                        .managed_drafts
                        .entries
                        .values()
                        .any(|view| view.blocked.is_some());
                shell.show_toast(if retained_version {
                    "A local draft version was saved separately. Review saved versions in Drafts."
                } else {
                    "Local drafts could not be saved. Keep the app open while saving retries."
                });
                changed = true;
            }
            draft_save_error = failed_account;
            let reactions = shell.huddle_ui.reactions.len();
            shell
                .huddle_ui
                .reactions
                .retain(|reaction| now.duration_since(reaction.shown_at).as_secs() < 5);
            changed |= reactions != shell.huddle_ui.reactions.len();
            let had_toast = shell.toast.is_some();
            shell.expire_toast(now);
            changed |= had_toast && shell.toast.is_none();
            let previous_status = shell.connection.status;
            connection = crate::connection::evaluate(&mut shell, now);
            changed |= previous_status != shell.connection.status;
            changed |= shell
                .core
                .huddle
                .expire_invites(now, crate::huddle::INVITE_RING);
        }
        let typing_changed = timing::prune_typing(shell.core.workspaces.values_mut(), now);
        let before = shell.message_arrivals.len();
        shell
            .message_arrivals
            .retain(|_, started: &mut std::time::Instant| {
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
            // Typing names are read directly from core during render; no channel
            // or message projection needs rebuilding when a typing entry expires.
            if uploading {
                shell.upload_ui_epoch = shell.upload_ui_epoch.wrapping_add(1);
            }
            changed = true;
        }
        // Bumping the generation is what re-stamps painted `src` attributes, so
        // images swap from their placeholder as soon as any bytes land — one
        // slow host cannot hold back the whole batch.
        if shell.media.take_dirty() {
            shell.media_epoch = shell.media_epoch.wrapping_add(1);
            changed = true;
        }
        // Nothing is swept into a link that is known to be down: every fetch
        // would fail, and the loader would spend the outage burning through
        // each picture's retry backoff.
        let sweep = media_sweep.due(now)
            && shell.media.has_pending()
            && !shell.media.is_loading()
            && shell
                .core
                .transport
                .as_ref()
                .is_none_or(|transport| transport.health() != Health::Offline);
        let transport = sweep.then(|| shell.core.transport.clone()).flatten();
        let close_pending = !shell.app_modal_closes.is_empty();
        let list_viewing = crate::slack_lists::lifecycle::plan(
            &mut shell,
            dioxus::desktop::window().window.is_focused(),
            crate::slack_lists::lifecycle::now_ms(),
        );
        drop(shell);
        for request in list_viewing {
            request.spawn(state);
        }
        if close_pending {
            crate::app_modal::flush_closes(state);
        }
        if changed {
            // A normal write notifies readers of the changes made above.
            drop(state.write());
        }
        match connection {
            crate::connection::Next::Nothing => {}
            crate::connection::Next::Probe => {
                dioxus::prelude::spawn(crate::connection::probe(state));
            }
            crate::connection::Next::Reload => {
                dioxus::prelude::spawn(crate::bootstrap::reload_after_outage(state));
            }
        }
        if let Some(transport) = transport {
            let media = state.read().media.clone();
            dioxus::prelude::spawn(async move { media.load_pending(transport).await });
        }
    }
}
