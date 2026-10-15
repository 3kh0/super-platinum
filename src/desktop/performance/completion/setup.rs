//! Fixture-only oracle setup, outside all input/result/RAF intervals.
use std::{future::Future, time::Duration};

pub(super) async fn collect<T>(
    count: usize,
    compute: impl FnMut(usize) -> T,
    current: impl FnMut() -> bool,
) -> Result<Vec<T>, String> {
    collect_with_pause(count, compute, current, || {
        tokio::time::sleep(Duration::from_millis(1))
    })
    .await
}

async fn collect_with_pause<T, F: Future<Output = ()>>(
    count: usize,
    mut compute: impl FnMut(usize) -> T,
    mut current: impl FnMut() -> bool,
    mut pause: impl FnMut() -> F,
) -> Result<Vec<T>, String> {
    let mut values = Vec::with_capacity(count);
    for index in 0..count {
        // This check also runs after every timer await, before touching state.
        if !current() {
            return Err("Owner changed during expected-case setup".into());
        }
        values.push(compute(index));
        if index + 1 < count {
            // yield_now may be repolled immediately. A bounded timer leaves this
            // future pending so the native loop can service its window messages.
            // There are exactly count-1 waits; none persist into the measured run.
            pause().await;
        }
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        cell::Cell,
        future::Future,
        task::{Context, Poll, Waker},
    };

    #[tokio::test]
    async fn does_not_compute_a_second_case_before_yielding_and_rechecks_owner() {
        let calls = Cell::new(0);
        let owner = Cell::new(true);
        let released = tokio::sync::Notify::new();
        let mut future = Box::pin(collect_with_pause(
            24,
            |index| {
                calls.set(calls.get() + 1);
                index
            },
            || owner.get(),
            || released.notified(),
        ));
        let mut context = Context::from_waker(Waker::noop());
        assert!(matches!(future.as_mut().poll(&mut context), Poll::Pending));
        assert_eq!(calls.get(), 1);
        owner.set(false);
        released.notify_one();
        assert_eq!(
            future.await.unwrap_err(),
            "Owner changed during expected-case setup"
        );
        assert_eq!(calls.get(), 1);
    }

    #[tokio::test]
    async fn stable_owner_preserves_every_case_in_original_order() {
        let checks = Cell::new(0);
        let values = collect(
            24,
            |index| index,
            || {
                checks.set(checks.get() + 1);
                true
            },
        )
        .await
        .unwrap();
        assert_eq!(values, (0..24).collect::<Vec<_>>());
        assert_eq!(checks.get(), 24);
    }

    #[tokio::test]
    async fn rejects_stale_owner_before_any_computation() {
        let result = collect(24, |_| panic!("stale owner computed a case"), || false).await;
        assert!(result.is_err());
    }
}
