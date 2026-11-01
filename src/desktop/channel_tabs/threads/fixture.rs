//! Disabled presentation only. No session, assignment, canonical or route grant.
use super::*;
use crate::state::MessageVm;
#[derive(Clone)]
pub(crate) struct Item {
    pub root: MessageVm,
    pub replies: Vec<MessageVm>,
    pub earlier: bool,
    pub previous: bool,
}
#[derive(Clone)]
pub(crate) struct Presentation {
    pub items: Vec<Item>,
    pub notice: String,
    pub loading: bool,
    pub has_more: bool,
}
pub(crate) fn install(shell: &mut ShellState, name: &str) {
    shell.channel_tab = crate::state::ChannelTab::Threads;
    let root=MessageVm{id:"1710000000.000001".into(),ts:"1710000000.000001".into(),author:"Morgan Chen".into(),timestamp:"10:24 AM".into(),avatar_initials:"MC".into(),body:vec![crate::state::RichNode::Paragraph(vec![crate::state::RichNode::StyledText{text:"Release checklist".into(),bold:true,italic:false,strike:false,code:false}]),crate::state::RichNode::Paragraph(vec![crate::state::RichNode::Text("The updated handoff is ready for review. Keep the rollout notes together in this thread.".into())])],reply_count:147,..Default::default()};
    let reply = MessageVm {
        id: "1710000001.000001".into(),
        ts: "1710000001.000001".into(),
        author: "Avery Patel".into(),
        timestamp: "10:28 AM".into(),
        avatar_initials: "AP".into(),
        body: vec![crate::state::RichNode::Text(
            "Reviewed the checklist. The remaining follow-up is documented in the launch notes."
                .into(),
        )],
        ..Default::default()
    };
    let empty = name.contains("empty");
    let loading = name.contains("loading");
    let failed = name.contains("error");
    shell.channel_tabs.threads.presentation = Some(Presentation {
        items: if empty || loading {
            Vec::new()
        } else {
            vec![Item {
                root,
                replies: if name.contains("previous") {
                    (0..12)
                        .map(|i| {
                            let mut row = reply.clone();
                            row.id = format!("17100000{:02}.000001", i + 1);
                            row.ts = row.id.clone();
                            row
                        })
                        .collect()
                } else {
                    vec![reply]
                },
                earlier: true,
                previous: name.contains("previous"),
            }]
        },
        notice: if failed {
            "Could not refresh threads. Your loaded messages are still available.".into()
        } else {
            String::new()
        },
        loading,
        has_more: !empty,
    });
}
pub(crate) fn presentation(shell: &ShellState) -> Option<&Presentation> {
    crate::fixture::is_fixture().then_some(())?;
    shell.channel_tabs.threads.presentation.as_ref()
}
