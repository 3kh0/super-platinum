use super::*;
use crate::drafts::protected::operation::tests::Fixture;
#[tokio::test]
async fn changed_native_adapter_propagates_actual_editor_refusal() {
    let fixture = Fixture::new().await;
    let operation = fixture.operation().await;
    let editor = fixture
        .account
        .retained_editors()
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let old = (*operation.record().local).clone();
    let current: super::super::protected_projection::NativeSnapshot = serde_json::from_value(serde_json::json!({
        "id":"channel-composer", "scope":"fixture", "visit":"captured", "wire":"Newer native", "input":2,
        "composition":0,"composing":false,"start":0,"end":12,"direction":"forward", "display_start":0,"display_end":12,"display_len":12
    })).unwrap();
    operation.native(&current, true).unwrap();
    let result =
        preserve_native_changed(&editor, &old, current, operation.record().serial + 1).await;
    assert_eq!(result, Err(Error::Stale));
    assert_eq!(operation.preservation(result), Err(Error::Stale));
    assert_eq!(
        fixture.owner.review().await.unwrap().local().composer.text,
        "Original"
    );
    let evidence = operation.record().evidence.lock().unwrap();
    assert_eq!(evidence.preservation, Some(Err(Error::Stale)));
    assert_eq!(
        evidence.preservation_input.as_ref().unwrap().wire,
        "Newer native"
    );
}
