//! Synthetic, credential-free sidebar projection benchmark.
//! Run with `cargo run --locked --example sidebar_benchmark`.
use std::{hint::black_box, time::Instant};
use super_platinum_core::{
    config::WorkspaceSession,
    slack::models::{Channel, Message},
    state::{ChannelMessages, SidebarMemory, Workspace, grouped_sidebar_sections},
};

fn main() {
    let mut workspace = Workspace::from_session(&WorkspaceSession {
        team_id: "T_BENCH".into(),
        enterprise_id: None,
        user_id: "U_BENCH".into(),
        name: "Synthetic benchmark".into(),
        url: "https://example.invalid".into(),
        token: String::new(),
    });
    for channel in 0..1_000 {
        let id = format!("C{channel:05}");
        workspace.channels.insert(
            id.clone(),
            Channel {
                id: id.clone(),
                name: Some(format!("channel-{channel:05}")),
                is_channel: true,
                ..Default::default()
            },
        );
        workspace.messages.insert(
            id,
            ChannelMessages {
                unread_count: 1,
                messages: (0..200)
                    .map(|message| Message {
                        ts: Some(format!("170000{:04}.000001", message)),
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            },
        );
    }
    let mut memory = SidebarMemory::default();
    for sort in ["default", "recent", "priority"] {
        workspace.sidebar.channel_sort = sort.into();
        let expected = grouped_sidebar_sections(&workspace, None, &mut memory);
        let rows: usize = expected
            .iter()
            .map(|section| section.channel_ids.len())
            .sum();
        assert_eq!(rows, 1_000);
        let mut times = Vec::new();
        for _ in 0..25 {
            let start = Instant::now();
            let result = grouped_sidebar_sections(black_box(&workspace), None, &mut memory);
            times.push(start.elapsed().as_secs_f64() * 1_000.0);
            assert_eq!(result, expected);
            black_box(result);
        }
        times.sort_by(f64::total_cmp);
        println!(
            "sort={sort} channels=1000 messages=200000 runs=25 median_ms={:.3} p95_ms={:.3}",
            times[12], times[23]
        );
    }
}
