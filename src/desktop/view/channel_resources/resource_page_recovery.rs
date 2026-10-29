use super::*;
use crate::resource_reads::render_test::{button, click, mount, settle, text};
fn render(state: Signal<ShellState>) -> Element {
    let channel = state.peek().core.active_channel.clone().unwrap();
    LinksPanel(LinksPanelProps { state, channel })
}
#[cfg(feature = "protected-native-fixture")]
#[tokio::test(flavor = "current_thread")]
async fn generated_links_controls_keep_loaded_pagination_retry_exact_target_and_disable_busy_navigation()
 {
    for alias in ["resource-links-recovery", "resource-links-recovery-narrow"] {
        let (mut dom, mut state, captured) = mount(alias, render);
        assert!(text(captured.borrow().as_ref().unwrap()).contains("Page 1 of 4"));
        assert!(
            text(captured.borrow().as_ref().unwrap()).contains("Showing the last loaded results.")
        );
        let before = state
            .peek()
            .core
            .channel_links
            .items
            .iter()
            .map(|l| l.url.clone())
            .collect::<Vec<_>>();
        for id in ["links-retry", "links-next"] {
            let generation = state.peek().core.channel_links.generation;
            let control = button(captured.borrow().as_ref().unwrap(), id).unwrap();
            click(&dom, control);
            settle(&mut dom, || {
                state.peek().core.channel_links.generation != generation
                    && !state.peek().core.channel_links.loading
            })
            .await;
            assert_eq!(
                state.peek().core.channel_links.requested_page,
                Some(if id == "links-retry" { 3 } else { 2 })
            );
            assert_eq!(state.peek().core.channel_links.page, 1);
            assert_eq!(
                state
                    .peek()
                    .core
                    .channel_links
                    .items
                    .iter()
                    .map(|l| l.url.clone())
                    .collect::<Vec<_>>(),
                before
            );
        }
        let retired = button(captured.borrow().as_ref().unwrap(), "links-next").unwrap();
        let generation = state.peek().core.channel_links.generation;
        {
            let mut shell = state.write();
            shell.channel_tab = ChannelTab::Messages;
            shell.channel_generation += 1;
            shell.channel_tab = ChannelTab::Links;
            shell.channel_generation += 1;
        }
        assert!(!retired.disabled);
        click(&dom, retired);
        let _ = tokio::time::timeout(std::time::Duration::from_millis(50), async {
            loop {
                dom.render_immediate_to_vec();
                dom.wait_for_work().await;
            }
        })
        .await;
        assert_eq!(state.peek().core.channel_links.generation, generation);
        {
            let mut shell = state.write();
            let query = shell.core.channel_links.query.clone();
            shell.core.channel_links.begin(query, 2);
        }
        dom.render_immediate_to_vec();
        assert!(
            button(captured.borrow().as_ref().unwrap(), "links-next")
                .unwrap()
                .disabled
        );
        assert!(text(captured.borrow().as_ref().unwrap()).contains("Page 1 of 4"));
        {
            let mut shell = state.write();
            let generation = shell.core.channel_links.begin("new empty query".into(), 1);
            shell.core.channel_links.fail(generation, "Offline");
        }
        dom.render_immediate_to_vec();
        assert!(state.peek().core.channel_links.items.is_empty());
        assert!(!state.peek().core.channel_links.loaded);
        assert!(
            !button(captured.borrow().as_ref().unwrap(), "links-retry")
                .unwrap()
                .disabled
        );
        assert!(
            button(captured.borrow().as_ref().unwrap(), "links-next")
                .unwrap()
                .disabled
        );
        assert!(!text(captured.borrow().as_ref().unwrap()).contains("Page "));
        let generation = state.peek().core.channel_links.generation;
        let retry = button(captured.borrow().as_ref().unwrap(), "links-retry").unwrap();
        click(&dom, retry);
        settle(&mut dom, || {
            state.peek().core.channel_links.generation != generation
                && !state.peek().core.channel_links.loading
        })
        .await;
        assert_eq!(state.peek().core.channel_links.requested_page, Some(1));
        assert!(!state.peek().core.channel_links.loaded);
        assert_eq!(state.peek().core.composer.text, "Keep resource draft");
    }
}
