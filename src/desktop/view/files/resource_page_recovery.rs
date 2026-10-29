use super::*;
use crate::resource_reads::render_test::{button, click, mount, settle, text};
fn render(state: Signal<ShellState>) -> Element {
    FilesPanel(FilesPanelProps {
        state,
        channel: None,
    })
}
#[cfg(feature = "protected-native-fixture")]
#[tokio::test(flavor = "current_thread")]
async fn generated_files_controls_keep_committed_page_and_retry_target_and_refuse_retired_callbacks()
 {
    for alias in ["resource-files-recovery", "resource-files-recovery-narrow"] {
        let (mut dom, mut state, captured) = mount(alias, render);
        {
            let rendered = captured.borrow();
            let rendered = rendered.as_ref().unwrap();
            assert!(text(rendered).contains("Page 1 of 4"));
            assert!(text(rendered).contains("Showing the last loaded results."));
            assert!(!button(rendered, "files-next").unwrap().disabled);
            assert!(!button(rendered, "files-retry").unwrap().disabled);
        }
        let before = state
            .peek()
            .core
            .files
            .items
            .iter()
            .map(|f| f.id.clone())
            .collect::<Vec<_>>();
        for id in ["files-retry", "files-next"] {
            let generation = state.peek().core.files.generation;
            let control = button(captured.borrow().as_ref().unwrap(), id).unwrap();
            click(&dom, control);
            settle(&mut dom, || {
                state.peek().core.files.generation != generation && !state.peek().core.files.loading
            })
            .await;
            assert_eq!(
                state.peek().core.files.requested_page,
                Some(if id == "files-retry" { 3 } else { 2 })
            );
            assert_eq!(state.peek().core.files.page, 1);
            assert_eq!(
                state
                    .peek()
                    .core
                    .files
                    .items
                    .iter()
                    .map(|f| f.id.clone())
                    .collect::<Vec<_>>(),
                before
            );
        }
        let refresh = button(captured.borrow().as_ref().unwrap(), "files-refresh").unwrap();
        let generation = state.peek().core.files.generation;
        click(&dom, refresh);
        settle(&mut dom, || {
            state.peek().core.files.generation != generation && !state.peek().core.files.loading
        })
        .await;
        assert_eq!(
            state.peek().core.files.requested_page,
            Some(1),
            "Refresh owns committed page rather than failed second page"
        );
        let retired = button(captured.borrow().as_ref().unwrap(), "files-next").unwrap();
        let generation = state.peek().core.files.generation;
        {
            let mut shell = state.write();
            shell.main_view = crate::state::MainView::Home;
            shell.close_thread();
            shell.main_view = crate::state::MainView::Files;
            shell.close_thread();
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
        assert_eq!(state.peek().core.files.generation, generation);
        {
            let mut shell = state.write();
            let query = shell.core.files.query.clone();
            shell.core.files.begin(query, 2);
        }
        dom.render_immediate_to_vec();
        assert!(
            button(captured.borrow().as_ref().unwrap(), "files-next")
                .unwrap()
                .disabled
        );
        assert!(text(captured.borrow().as_ref().unwrap()).contains("Page 1 of 4"));
        {
            let mut shell = state.write();
            let generation = shell.core.files.begin(
                super_platinum_core::files::FilesQuery {
                    text: "new empty query".into(),
                    ..Default::default()
                },
                1,
            );
            shell.core.files.fail(generation, "Offline");
        }
        dom.render_immediate_to_vec();
        assert!(state.peek().core.files.items.is_empty());
        assert!(!state.peek().core.files.loaded);
        assert!(
            !button(captured.borrow().as_ref().unwrap(), "files-retry")
                .unwrap()
                .disabled
        );
        assert!(
            button(captured.borrow().as_ref().unwrap(), "files-next")
                .unwrap()
                .disabled
        );
        assert!(!text(captured.borrow().as_ref().unwrap()).contains("Page "));
        let generation = state.peek().core.files.generation;
        let retry = button(captured.borrow().as_ref().unwrap(), "files-retry").unwrap();
        click(&dom, retry);
        settle(&mut dom, || {
            state.peek().core.files.generation != generation && !state.peek().core.files.loading
        })
        .await;
        assert_eq!(state.peek().core.files.requested_page, Some(1));
        assert!(!state.peek().core.files.loaded);
        assert_eq!(state.peek().core.composer.text, "Keep resource draft");
    }
}
