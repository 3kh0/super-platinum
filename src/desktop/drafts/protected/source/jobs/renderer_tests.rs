//! Existing renderer executor, actual Client calls, synthetic socket only.
use super::*;
use dioxus::prelude::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

fn shell_signal(shell: ShellState) -> (VirtualDom, Signal<ShellState>) {
    type Props = (
        Rc<RefCell<Option<ShellState>>>,
        Rc<Cell<Option<Signal<ShellState>>>>,
    );
    let slot = Rc::new(Cell::new(None));
    let mut dom = VirtualDom::new_with_props(
        |(shell, slot): Props| {
            let state = use_signal(|| shell.borrow_mut().take().unwrap());
            slot.set(Some(state));
            VNode::empty()
        },
        (Rc::new(RefCell::new(Some(shell))), slot.clone()),
    );
    dom.rebuild_in_place();
    (dom, slot.get().unwrap())
}
fn request(f: &Fixture, state: Signal<ShellState>) -> Request {
    Request {
        state,
        account: f.account.clone(),
        socket: f.socket.clone(),
        frame_generation: 9,
        team: "T1".into(),
    }
}
async fn receive(dom: &mut VirtualDom, opening: Opening) -> Arc<Source> {
    let result = opening.receive();
    tokio::pin!(result);
    tokio::time::timeout(std::time::Duration::from_secs(5),async {
        loop {tokio::select! {r=&mut result=>break r.unwrap(),_=dom.wait_for_work()=>{dom.render_immediate_to_vec();}}}
    }).await.unwrap()
}

#[tokio::test]
async fn actual_renderer_open_retains_replies_then_delivers_same_allocation() {
    let mut f = Box::pin(Fixture::new()).await;
    let replacement = ShellState::opening(crate::media::MediaRegistry::default());
    let (mut dom, state) = shell_signal(std::mem::replace(&mut f.shell, replacement));
    let opening = dom
        .in_runtime(|| request(&f, state).begin())
        .unwrap_or_else(|_| panic!("reserve"));
    assert_eq!(f.account.retained_source_openings().unwrap().len(), 1);
    let source = receive(&mut dom, opening).await;
    let h = f.account.retained_source_openings().unwrap();
    assert!(Arc::ptr_eq(&h[0].source().unwrap(), &source));
    let held = h[0].job.state.lock().unwrap();
    assert!(matches!(
        held.read.as_ref().unwrap().result(),
        Ok(Some(Ok(_)))
    ));
    assert!(matches!(
        held.open.as_ref().unwrap().result(),
        Ok(Some(Ok(_)))
    ));
    assert_eq!(held.phase, Phase::Delivered);
    drop(held);
    drop(source);
    settled_gap(&h[0].job).await.unwrap();
    f.account.acknowledge_source_opening(&h[0]).unwrap();
}

#[tokio::test]
async fn renderer_canceled_before_first_poll_retains_original_unsubmitted_job() {
    let mut f = Box::pin(Fixture::new()).await;
    let replacement = ShellState::opening(crate::media::MediaRegistry::default());
    let (dom, state) = shell_signal(std::mem::replace(&mut f.shell, replacement));
    let opening = dom
        .in_runtime(|| request(&f, state).begin())
        .unwrap_or_else(|_| panic!("reserve"));
    let h = f.account.retained_source_openings().unwrap();
    drop(dom);
    assert!(matches!(
        opening.receive().await,
        Err(durable::Error::Storage)
    ));
    let held = h[0].job.state.lock().unwrap();
    assert!(held.abandoned);
    assert!(held.read.is_none() && held.open.is_none() && held.issued.is_none());
    drop(held);
    assert!(h[0].job.gap.attempts().is_empty());
    f.account.acknowledge_source_opening(&h[0]).unwrap();
}

#[tokio::test]
async fn full_book_returns_exact_request_without_any_new_issuance() {
    let mut f = Box::pin(Fixture::new()).await;
    for _ in 0..MAX_JOBS {
        f.reserve();
    }
    let replacement = ShellState::opening(crate::media::MediaRegistry::default());
    let (dom, state) = shell_signal(std::mem::replace(&mut f.shell, replacement));
    let expected = request(&f, state);
    let ptr = expected.team.as_ptr();
    let before = f.account.source_jobs_inventory().unwrap();
    let refused = match dom.in_runtime(|| expected.begin()) {
        Err(v) => v,
        Ok(_) => panic!("must refuse"),
    };
    assert_eq!(refused.error, durable::Error::Pending);
    assert_eq!(refused.request.team.as_ptr(), ptr);
    assert!(Arc::ptr_eq(&refused.request.account, &f.account));
    assert!(Arc::ptr_eq(&refused.request.socket, &f.socket));
    assert!(refused.request.state == state);
    assert_eq!(refused.request.frame_generation, 9);
    before.validate(&f.account).unwrap();
}

#[test]
fn no_runtime_returns_original_request_before_reservation() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut f = runtime.block_on(Box::pin(Fixture::new()));
    let replacement = ShellState::opening(crate::media::MediaRegistry::default());
    let (dom, state) = shell_signal(std::mem::replace(&mut f.shell, replacement));
    let expected = request(&f, state);
    let ptr = expected.team.as_ptr();
    let refused = match dom.in_runtime(|| expected.begin()) {
        Err(v) => v,
        Ok(_) => panic!("runtime required"),
    };
    assert_eq!(refused.error, durable::Error::Storage);
    assert_eq!(refused.request.team.as_ptr(), ptr);
    assert!(Arc::ptr_eq(&refused.request.account, &f.account));
    assert!(Arc::ptr_eq(&refused.request.socket, &f.socket));
    assert!(f.account.retained_source_openings().unwrap().is_empty());
}
