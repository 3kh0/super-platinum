use crate::{media::MediaRegistry, state::ShellState};

#[test]
fn normal_and_narrow_search_conversation_fixtures_keep_channel_and_direct_headings_distinct() {
    for name in ["search-conversations", "search-conversations-narrow"] {
        let mut shell = ShellState::fixture(MediaRegistry::default());
        crate::bootstrap::search_fixture(&mut shell, name);
        let headings = shell
            .search_results
            .iter()
            .map(crate::view::search::result_heading)
            .collect::<Vec<_>>();
        assert_eq!(
            headings,
            [
                "# ship · Jules",
                "Maya Chen · Jules",
                "Maya Chen, Jules · Jules"
            ]
        );
        assert_eq!(
            shell
                .search_results
                .iter()
                .map(|r| r.direct_conversation)
                .collect::<Vec<_>>(),
            [false, true, true]
        );
        assert_eq!((shell.search_pager.page, shell.search_pager.total), (1, 3));
        assert!(!shell.search_pager.loading && !shell.search_pager.has_more());
    }
}
