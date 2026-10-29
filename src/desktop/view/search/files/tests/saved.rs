use super::*;
use pages::saved::{self, test_io::Io as SaveIo};
use super_platinum_core::slack::transport::FileSortPreferenceFailure as Failure;
#[tokio::test]
async fn generated_saved_sort_saves_independently_in_both_completion_orders_and_retry_does_not_refetch()
 {
    static EVENTS: std::sync::Once = std::sync::Once::new();
    EVENTS.call_once(|| {
        dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter))
    });
    let h = saved::tests::loaded().await;
    let mut old = h.h.state;
    let shell = std::mem::replace(
        &mut *old.write(),
        ShellState::fixture(crate::media::MediaRegistry::default()),
    );
    type Input = Rc<RefCell<Option<ShellState>>>;
    type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
    let slot = Slot::default();
    let mut dom = VirtualDom::new_with_props(
        |(input, slot): (Input, Slot)| {
            let state = use_signal(|| input.borrow_mut().take().unwrap());
            slot.set(Some(state));
            rsx! {{super::super::super::controls(state,&state.read())}{results(state,&state.read())}{controls(state,&state.read())}}
        },
        (Rc::new(RefCell::new(Some(shell))), slot.clone()),
    );
    let mut attributes = Attributes::new();
    record(&mut attributes, dom.rebuild_to_vec().edits);
    let state = slot.get().unwrap();
    let reads = Rc::new(pages::test_io::Io::default());
    let saves = Rc::new(SaveIo::default());
    dom.in_scope(ScopeId::ROOT, || {
        provide_context(reads.clone());
        provide_context(saves.clone());
    });
    assert_eq!(pages::status(&state.peek()).unwrap().6, Sort::Newest);
    let read = reads.expect(1);
    let save = saves.expect(Sort::NameDescending);
    change(&dom, mounted_element(&dom, "id", SORT_ID), "name_desc");
    // The old rendered handler cannot start another read or save before rerender.
    change(&dom, mounted_element(&dom, "id", SORT_ID), "name");
    pump(&mut dom, &mut attributes, || {
        reads.pages.borrow().len() == 1 && saves.sorts.borrow().len() == 1
    })
    .await;
    assert!(saved::Source::capture(&state.peek()).is_none());
    save.send(Ok(serde_json::json!({"ok":true,"value":"score"})))
        .unwrap();
    pump(&mut dom, &mut attributes, || saves.completed.get() == 1).await;
    assert_eq!(
        saved::status(&state.peek()),
        Some("File sort preference submitted.")
    );
    assert!(
        pages::status(&state.peek()).unwrap().4,
        "write completion does not finish or replace independent pending read"
    );
    read.send(Ok(pages::tests::response(1, false))).unwrap();
    pump(&mut dom, &mut attributes, || {
        pages::status(&state.peek()).is_some_and(|v| !v.4)
    })
    .await;
    assert_eq!(
        pages::status(&state.peek()).unwrap().6,
        Sort::NameDescending
    );
    assert_eq!(&*reads.pages.borrow(), &[1]);
    let read = reads.expect(1);
    let save = saves.expect(Sort::Newest);
    change(&dom, mounted_element(&dom, "id", SORT_ID), "created");
    pump(&mut dom, &mut attributes, || {
        reads.pages.borrow().len() == 2 && saves.sorts.borrow().len() == 2
    })
    .await;
    read.send(Ok(pages::tests::response(1, false))).unwrap();
    pump(&mut dom, &mut attributes, || {
        pages::status(&state.peek()).is_some_and(|v| !v.4)
    })
    .await;
    assert!(saved::status(&state.peek()).unwrap().starts_with("Saving"));
    save.send(Err(Failure::Unconfirmed(
        super_platinum_core::slack::Error::Transport("synthetic lost reply".into()),
    )))
    .unwrap();
    pump(&mut dom, &mut attributes, || saves.completed.get() == 2).await;
    assert!(saved::status(&state.peek()).unwrap().contains("confirm"));
    assert_eq!(pages::rows(&state.peek()).len(), 3);
    let retry = saves.expect(Sort::Newest);
    click(&dom, mounted_element(&dom, "id", "file-search-save-retry"));
    pump(&mut dom, &mut attributes, || {
        saves.sorts.borrow().len() == 3
    })
    .await;
    retry
        .send(Ok(
            serde_json::json!({"ok":true,"prefs":{"search_file_sort":"name_desc"}}),
        ))
        .unwrap();
    pump(&mut dom, &mut attributes, || saves.completed.get() == 3).await;
    assert_eq!(
        &*reads.pages.borrow(),
        &[1, 1],
        "no save outcome or retry launches an acknowledgment read"
    );
    assert_eq!(
        &*saves.sorts.borrow(),
        &[Sort::NameDescending, Sort::Newest, Sort::Newest]
    );
    assert_eq!(pages::status(&state.peek()).unwrap().6, Sort::Newest);
    assert!(state.peek().viewer.is_none());
}
