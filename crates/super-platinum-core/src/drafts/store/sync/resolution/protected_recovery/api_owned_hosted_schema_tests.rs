use super::*;

#[test]
fn actual_owned_job_survives_dropped_handle_and_requires_fresh_open_inspection() {
    let ticket = {
        let route = OwnedRoute::fresh_fixture().unwrap();
        let first = route.inspect().unwrap().wait().unwrap();
        assert!(route.initialize_hosted(&first).is_err());
        route.commit(&first).unwrap().wait().unwrap();
        let review = route.inspect().unwrap().wait().unwrap();
        let job = route.initialize_hosted(&review).unwrap();
        let id = job.id();
        drop(job);
        let receipt = route
            .resume_hosted_initialization(id.clone())
            .unwrap()
            .wait()
            .unwrap();
        assert!(receipt.changed);
        assert_eq!(
            route
                .resume_hosted_initialization(id)
                .unwrap()
                .wait()
                .unwrap(),
            receipt
        );
        assert!(review.open().is_err());
        let inspected = route.inspect().unwrap().wait().unwrap();
        let opened = inspected.open().unwrap();
        let ticket = opened.fixture_ticket().unwrap();
        drop(opened);
        drop(route);
        ticket
    };
    let reopened = OwnedRoute::reopen_fixture(&ticket).unwrap();
    let inspected = reopened.inspect().unwrap().wait().unwrap();
    assert!(
        !reopened
            .initialize_hosted(&inspected)
            .unwrap()
            .wait()
            .unwrap()
            .changed
    );
    let inspected = reopened.inspect().unwrap().wait().unwrap();
    inspected.open().unwrap();
    assert!(reopened.inspect().is_err());
}

#[test]
fn public_cross_route_job_and_review_are_refused_without_fallback() {
    let a = OwnedRoute::fresh_fixture().unwrap();
    let b = OwnedRoute::fresh_fixture().unwrap();
    let review = a.inspect().unwrap().wait().unwrap();
    a.commit(&review).unwrap().wait().unwrap();
    let review = a.inspect().unwrap().wait().unwrap();
    assert!(b.initialize_hosted(&review).is_err());
    let job = a.initialize_hosted(&review).unwrap();
    assert!(b.resume_hosted_initialization(job.id()).is_err());
    assert!(job.wait().unwrap().changed);
}
