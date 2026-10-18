//! Actual Root executor + same owned Account capacity. Browser projection is
//! explicitly represented by the operation test seam, not native evidence.
use super::*;
use crate::drafts::protected::operation::RecoveryFixture;

#[tokio::test]
async fn twenty_failed_baseline_projections_recover_same_root_account_without_erasing_status() {
    let mut fixture = RecoveryFixture::new().await;
    let identity = fixture.account_id();
    let root = Root::default();
    let foreign = Root::default();
    let mut history = Vec::new();
    for _ in 0..20 {
        assert_eq!(fixture.account_id(), identity);
        fixture.dom.in_runtime(|| root.has_capacity()).unwrap();
        let request = fixture.reserve().await;
        let operation = request.operation.clone();
        fixture
            .dom
            .in_runtime(|| root.install(request.operation, request.future))
            .unwrap();
        assert_eq!(root.entries.borrow().len(), 1);
        let result = fixture.drive(request.applied).await;
        assert_eq!(
            operation.ready().await.unwrap(),
            Status::Failed(durable::Error::ReviewRequired)
        );
        assert_eq!(fixture.retained().len(), 1);
        assert!(matches!(
            root.acknowledge_baseline(&operation),
            Err(durable::Error::Pending)
        ));
        fixture.observe_projection(&operation, &result);
        assert!(matches!(
            foreign.acknowledge_baseline(&operation),
            Err(durable::Error::Stale)
        ));
        assert_eq!(root.entries.borrow().len(), 1);
        assert_eq!(fixture.retained().len(), 1);
        assert_eq!(
            root.acknowledge_baseline(&operation).unwrap(),
            Status::Failed(durable::Error::ReviewRequired)
        );
        assert!(root.entries.borrow().is_empty());
        assert!(fixture.retained().is_empty());
        assert!(matches!(
            root.acknowledge_baseline(&operation),
            Err(durable::Error::Stale)
        ));
        history.push(operation);
    }
    assert_eq!(history.len(), 20);
    for original in history {
        assert_eq!(
            original.ready().await.unwrap(),
            Status::Failed(durable::Error::ReviewRequired)
        );
    }
}
