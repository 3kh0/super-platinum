//! Actual public route runtime without cfg(test) on the core library.
//! Fresh isolated ownership only; no configured account, session or network.
#![cfg(feature = "protected-test-support")]
use super_platinum_core::drafts::store::protected::owned_store::OwnedRoute;

#[test]
fn external_library_initializes_inspects_and_reopens_without_test_seed() {
    let ticket = {
        let route = OwnedRoute::fresh_fixture().unwrap();
        let review = route.inspect().unwrap().wait().unwrap();
        route.commit(&review).unwrap().wait().unwrap();
        let review = route.inspect().unwrap().wait().unwrap();
        assert!(
            route
                .initialize_slash(&review)
                .unwrap()
                .wait()
                .unwrap()
                .changed
        );
        let opened = route.inspect().unwrap().wait().unwrap().open().unwrap();
        let ticket = opened.fixture_ticket().unwrap();
        drop(opened);
        drop(route);
        ticket
    };
    let route = OwnedRoute::reopen_fixture(&ticket).unwrap();
    let review = route.inspect().unwrap().wait().unwrap();
    assert!(
        !route
            .initialize_slash(&review)
            .unwrap()
            .wait()
            .unwrap()
            .changed
    );
    route.inspect().unwrap().wait().unwrap().open().unwrap();
}
