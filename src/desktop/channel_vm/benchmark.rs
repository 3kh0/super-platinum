//! Disconnected comparison of cached-history projection and ordinary refresh.
use super::*;
use std::time::Instant;
fn measure(mut action: impl FnMut()) -> (f64, f64, f64) {
    let mut samples = Vec::new();
    for i in 0..11 {
        let start = Instant::now();
        action();
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        if i > 0 {
            samples.push(ms);
        }
    }
    samples.sort_by(f64::total_cmp);
    ((samples[4] + samples[5]) / 2.0, samples[0], samples[9])
}
#[test]
#[ignore = "manual isolated channel refresh measurement"]
fn mixed_history_refresh() {
    for count in [200, 2000, 10000] {
        let mut shell = crate::state::ShellState::fixture(MediaRegistry::default());
        let mut bag = crate::thread_vm::fixture_bag(count);
        bag.last_read = Some("1719800430.000000".into());
        shell
            .core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .messages
            .insert("C2".into(), bag);
        shell.core.active_channel = Some("C2".into());
        shell.refresh_from_core();
        let workspace = &shell.core.workspaces["T1"];
        let projection = measure(|| {
            std::hint::black_box(project_messages_for_channel(
                workspace,
                "C2",
                &shell.media,
                None,
            ));
        });
        assert_eq!(shell.channel_rows.len(), count + 1);
        assert_eq!(
            shell.messages.len(),
            shell.timeline_end - shell.timeline_start
        );
        assert!(
            shell.messages.len() <= 100,
            "rich projection must stay bounded"
        );
        let window_projection = measure(|| {
            std::hint::black_box(project_channel_window(
                workspace,
                "C2",
                &shell.media,
                &shell.channel_rows,
                shell.timeline_start..shell.timeline_end,
            ));
        });
        let clone = measure(|| {
            std::hint::black_box(shell.messages.clone());
        });
        let unchanged = measure(|| {
            shell.refresh_from_core();
            std::hint::black_box(&shell.messages);
        });
        let mut edit = 0usize;
        let changed = measure(|| {
            edit += 1;
            let message = shell
                .core
                .workspaces
                .get_mut("T1")
                .unwrap()
                .messages
                .get_mut("C2")
                .unwrap()
                .messages
                .last_mut()
                .unwrap();
            message.text = Some(format!("Updated reply {edit}"));
            message.blocks = vec![
                serde_json::json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[{"type":"text","text":format!("Updated reply {edit}")}]}]}),
            ];
            shell.refresh_from_core();
            std::hint::black_box(&shell.messages);
        });
        assert_eq!(shell.channel_rows.len(), count + 1);
        assert_eq!(
            shell.messages.len(),
            shell.timeline_end - shell.timeline_start
        );
        assert!(shell.messages.len() <= 100);
        println!(
            "channel_cached={} metadata_rows={} projected={} mounted_range={} projection_ms={:.3}/{:.3}/{:.3} window_projection_ms={:.3}/{:.3}/{:.3} clone_ms={:.3}/{:.3}/{:.3} unchanged_refresh_ms={:.3}/{:.3}/{:.3} edited_refresh_ms={:.3}/{:.3}/{:.3}",
            count + 1,
            shell.channel_rows.len(),
            shell.messages.len(),
            shell.timeline_end - shell.timeline_start,
            projection.0,
            projection.1,
            projection.2,
            window_projection.0,
            window_projection.1,
            window_projection.2,
            clone.0,
            clone.1,
            clone.2,
            unchanged.0,
            unchanged.1,
            unchanged.2,
            changed.0,
            changed.1,
            changed.2
        );
    }
}
