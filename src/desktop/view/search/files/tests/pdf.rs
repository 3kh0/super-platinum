//! Actual private row -> fresh PDF/IP/body -> mounted inert decoder admission.
//! No native mount, PDF rendering, OS input or live Slack acceptance is claimed.
use super::*;
use crate::pdf_preview::test_io::Io as Decoder;
use pages::pdf::{self as owned_pdf, Display, test_io::Io};
use serde_json::{Value, json};
const URL: &str = "https://files.slack.com/private-original.pdf?version=one";
fn page() -> Value {
    json!({"ok":true,"items":[{"id":"F12345678","title":"Launch PDF","pretty_type":"PDF"},{"id":"F12345678","title":"Another occurrence","pretty_type":"PDF"}],"pagination":{"page":1,"page_count":2,"total_count":3}})
}
fn metadata() -> Value {
    json!({"ok":true,"file":{"id":"F12345678","mode":"hosted","filetype":"pdf","mimetype":"application/pdf","url_private_download":URL,"converted_pdf":null,"file_access":"visible","is_hidden_by_limit":false,"is_tombstoned":false,"is_deleted":false,"restriction_type":0,"is_external":false,"external_type":null,"app_id":null,"created":1}})
}
fn mount(shell: ShellState) -> (VirtualDom, Signal<ShellState>, Attributes) {
    type Input = Rc<RefCell<Option<ShellState>>>;
    type Slot = Rc<Cell<Option<Signal<ShellState>>>>;
    let slot = Slot::default();
    let mut dom = VirtualDom::new_with_props(
        |(input, slot): (Input, Slot)| {
            let state = use_signal(|| input.borrow_mut().take().unwrap());
            slot.set(Some(state));
            rsx! {{results(state,&state.read())}{controls(state,&state.read())}}
        },
        (Rc::new(RefCell::new(Some(shell))), slot.clone()),
    );
    let mut attrs = Attributes::new();
    record(&mut attrs, dom.rebuild_to_vec().edits);
    (dom, slot.get().unwrap(), attrs)
}
async fn original_page(
    dom: &mut VirtualDom,
    a: &mut Attributes,
    state: Signal<ShellState>,
    io: &pages::test_io::Io,
) {
    let n = io.pages.borrow().len();
    let reply = io.expect(1);
    let source = pages::EntrySource::capture(&state.peek()).unwrap();
    dom.in_scope(ScopeId::ROOT, || {
        pages::change_sort(
            state,
            source,
            super_platinum_core::slack::api::file_pages::Sort::Newest,
        )
    });
    pump(dom, a, || io.pages.borrow().len() == n + 1).await;
    reply.send(Ok(page())).unwrap();
    pump(dom, a, || {
        pages::status(&state.peek()).is_some_and(|v| !v.4)
    })
    .await;
}
async fn prepare(
    dom: &mut VirtualDom,
    a: &mut Attributes,
    state: Signal<ShellState>,
    io: &Io,
    control: &str,
    ip_allowed: bool,
    body: Option<Vec<u8>>,
) {
    let n = io.calls.borrow().len();
    let m = io.expect(false);
    let ip = io.expect(true);
    let b = body.as_ref().map(|_| io.expect_bytes());
    let before = io.urls.borrow().len();
    click(dom, mounted_element(dom, "id", control));
    pump(dom, a, || io.calls.borrow().len() == n + 1).await;
    m.send(Ok(metadata())).unwrap();
    pump(dom, a, || io.calls.borrow().len() == n + 2).await;
    ip.send(Ok(json!({"ok":true,"ip_allowed":ip_allowed})))
        .unwrap();
    if let Some(body) = body {
        pump(dom, a, || io.urls.borrow().len() == before + 1).await;
        b.unwrap().send(Ok(body)).unwrap();
    }
    pump(dom, a, || {
        !matches!(owned_pdf::display(&state.peek(), 0), Some(Display::Loading))
    })
    .await;
}
fn mounted_decoder(dom: &VirtualDom, io: &Decoder) {
    let v = io.rendered().expect("actual IsolatedPdf body");
    let attr = v
        .dynamic_attrs
        .iter()
        .flat_map(|a| a.iter())
        .find(|a| a.name == "onmounted")
        .expect("actual decoder mount listener");
    let AttributeValue::Listener(callback) = attr.value.clone() else {
        panic!("mounted listener")
    };
    // The pinned serialized converter supplies MountedData::from(()), never a
    // native backing. The component's mandatory model IO remains explicit.
    dom.in_scope(ScopeId::ROOT, || {
        callback.call(dioxus::core::Event::new(
            dioxus::html::EventData::Mouse(dioxus::html::SerializedMouseData::default()).into_any(),
            false,
        ))
    });
}
fn text(dom: &VirtualDom) -> String {
    fn vnode(dom: &VirtualDom, v: &VNode, out: &mut String) {
        for n in v.template.roots {
            node(dom, v, n, out)
        }
    }
    fn node(dom: &VirtualDom, v: &VNode, n: &TemplateNode, out: &mut String) {
        match n {
            TemplateNode::Text { text } => {
                out.push_str(text);
                out.push(' ');
            }
            TemplateNode::Element { children, .. } => {
                for n in *children {
                    node(dom, v, n, out)
                }
            }
            TemplateNode::Dynamic { id } => match &v.dynamic_nodes[*id] {
                DynamicNode::Text(t) => {
                    out.push_str(&t.value);
                    out.push(' ');
                }
                DynamicNode::Fragment(nodes) => {
                    for n in nodes {
                        vnode(dom, n, out)
                    }
                }
                DynamicNode::Component(c) => {
                    if let Some(scope) = c.mounted_scope(*id, v, dom) {
                        vnode(dom, scope.root_node(), out)
                    }
                }
                _ => (),
            },
        }
    }
    let mut out = String::new();
    vnode(dom, dom.base_scope().root_node(), &mut out);
    out
}
#[tokio::test]
async fn isolated_search_pdf_actual_row_fresh_policy_private_bytes_decoder_and_retirement_journey()
{
    dioxus::html::set_event_converter(Box::new(dioxus::html::SerializedHtmlEventConverter));
    for name in [
        "search-files-pdf-preview",
        "search-files-pdf-preview-narrow",
    ] {
        let (dom, s, _) = mount(ShellState::fixture_variant(
            crate::media::MediaRegistry::default(),
            name,
        ));
        let shell = s.peek();
        assert!(shell.search_file_pdf_fixture);
        assert!(shell.core.session.is_none());
        assert!(shell.core.transport.is_none());
        assert!(shell.core.active_account.is_none());
        assert!(shell.viewer.is_none());
        assert!(owned_pdf::Source::capture(&shell, 0).is_none());
        assert!(owned_pdf::bytes(&shell, "fixture", "fixture").is_none());
        let copy = text(&dom);
        assert!(copy.contains("Close PDF preview"));
        for excluded in [
            "Download",
            "Share",
            "Rename",
            "Copy link",
            "Open in browser",
            "Print",
        ] {
            assert!(!copy.contains(excluded), "{excluded}");
        }
    }
    let original = pages::tests::setup(true).await;
    let mut old = original.h.state;
    let shell = std::mem::replace(
        &mut *old.write(),
        ShellState::fixture(crate::media::MediaRegistry::default()),
    );
    let (mut dom, mut state, mut attrs) = mount(shell);
    let pages_io = Rc::new(pages::test_io::Io::default());
    let io = Rc::new(Io::default());
    let decoder = Decoder::default();
    dom.in_scope(ScopeId::ROOT, || {
        provide_context(pages_io.clone());
        provide_context(io.clone());
        provide_context(decoder.clone());
    });
    assert!(
        owned_pdf::Source::capture(&state.peek(), 0).is_none(),
        "legacy/public rows cannot acquire the new purpose"
    );
    original_page(&mut dom, &mut attrs, state, &pages_io).await;
    let rows = pages::rows(&state.peek())
        .iter()
        .map(|h| (h.id().to_owned(), h.title().to_owned()))
        .collect::<Vec<_>>();
    let query = state.peek().search_query.clone();
    let channel = state.peek().core.active_channel.clone();
    let first = mounted_element(&dom, "id", "file-pdf-preview-0");
    let m = io.expect(false);
    click(&dom, first);
    click(&dom, first);
    pump(&mut dom, &mut attrs, || io.calls.borrow().len() == 1).await;
    m.send(Err(super_platinum_core::slack::Error::Offline(
        "synthetic PDF outage".into(),
    )))
    .unwrap();
    pump(&mut dom, &mut attrs, || {
        matches!(owned_pdf::display(&state.peek(), 0), Some(Display::Failed))
    })
    .await;
    assert_eq!(pages::rows(&state.peek()).len(), 2);
    assert_eq!(
        io.calls.borrow().len(),
        1,
        "queued peer issues no second request"
    );
    prepare(
        &mut dom,
        &mut attrs,
        state,
        &io,
        "file-pdf-retry-0",
        false,
        None,
    )
    .await;
    assert!(matches!(
        owned_pdf::display(&state.peek(), 0),
        Some(Display::Unavailable)
    ));
    assert!(io.urls.borrow().is_empty(), "IPfalse never fetches content");
    prepare(
        &mut dom,
        &mut attrs,
        state,
        &io,
        "file-pdf-retry-0",
        true,
        Some(b"not a PDF".to_vec()),
    )
    .await;
    assert!(matches!(
        owned_pdf::display(&state.peek(), 0),
        Some(Display::Unavailable)
    ));
    assert!(decoder.configs().is_empty());
    let document = crate::pdf_preview::fixture::document();
    prepare(
        &mut dom,
        &mut attrs,
        state,
        &io,
        "file-pdf-retry-0",
        true,
        Some(document.clone()),
    )
    .await;
    let Display::Document { id, nonce, ready } = owned_pdf::display(&state.peek(), 0).unwrap()
    else {
        panic!("fresh container stages only")
    };
    assert!(!ready, "container precheck is not decode/ready");
    assert_eq!(
        &*owned_pdf::bytes(&state.peek(), &nonce, id.opaque()).unwrap(),
        document.as_slice()
    );
    assert_eq!(
        &*crate::pdf_preview::search_bytes(&state.peek(), &format!("{nonce}/{}", id.opaque()))
            .unwrap(),
        document.as_slice()
    );
    assert!(
        crate::pdf_preview::search_bytes(&state.peek(), &format!("{nonce}/{}/extra", id.opaque()))
            .is_none()
    );
    assert!(owned_pdf::bytes(&state.peek(), "wrong-nonce", id.opaque()).is_none());
    assert!(
        state.peek().media.ready_bytes(&id).is_none(),
        "private staging is not generic media authority"
    );
    assert!(state.peek().viewer.is_none());
    assert_eq!(state.peek().overlay, Some(crate::state::Overlay::Search));
    let mut peer = decoder.bridge();
    let before = decoder.configs().len();
    mounted_decoder(&dom, &decoder);
    pump(&mut dom, &mut attrs, || {
        decoder.configs().len() == before + 1
    })
    .await;
    let c = decoder.configs().last().unwrap().clone();
    assert_eq!(
        c.source,
        format!("/pdf/search-data/{nonce}/{}", id.opaque())
    );
    peer.to_rust
        .send(json!({"kind":"decoded","id":c.id,"nonce":c.nonce,"pages":3}))
        .unwrap();
    pump(&mut dom, &mut attrs, || !peer.from_rust.is_empty()).await;
    assert_eq!(peer.from_rust.try_recv().unwrap()["kind"], "admit");
    assert_eq!(
        owned_pdf::decoded_fact(&state.peek(), 0),
        Some((3, false)),
        "model cannot mint native decode fact"
    );
    let copy = text(&dom);
    for excluded in [
        "Download",
        "Share",
        "Rename",
        "Copy link",
        "Open in browser",
        "Print",
    ] {
        assert!(!copy.contains(excluded), "{excluded}");
    }
    assert_eq!(state.peek().search_query, query);
    assert_eq!(state.peek().core.active_channel, channel);
    assert_eq!(
        pages::rows(&state.peek())
            .iter()
            .map(|h| (h.id().to_owned(), h.title().to_owned()))
            .collect::<Vec<_>>(),
        rows
    );
    assert_eq!(&*io.urls.borrow(), &[URL.to_owned(), URL.to_owned()]);
    peer.to_rust
        .send(json!({"kind":"closed","id":c.id,"nonce":c.nonce}))
        .unwrap();
    pump(&mut dom, &mut attrs, || {
        matches!(owned_pdf::display(&state.peek(), 0), Some(Display::Failed))
    })
    .await;
    assert!(
        owned_pdf::bytes(&state.peek(), &nonce, id.opaque()).is_none(),
        "spontaneous decoder close revokes its data and exposes recovery"
    );
    assert!(owned_pdf::decoded_fact(&state.peek(), 0).is_none());
    click(&dom, mounted_element(&dom, "id", "file-pdf-close-0"));
    record(&mut attrs, dom.render_immediate_to_vec().edits);
    assert!(owned_pdf::bytes(&state.peek(), &nonce, id.opaque()).is_none());
    assert!(owned_pdf::display(&state.peek(), 0).is_none());
    drop(peer);
    let n = io.calls.borrow().len();
    let completed = io.completed.get();
    let pending = io.expect(false);
    click(&dom, mounted_element(&dom, "id", "file-pdf-preview-1"));
    pump(&mut dom, &mut attrs, || io.calls.borrow().len() == n + 1).await;
    assert!(
        owned_pdf::display(&state.peek(), 0).is_none(),
        "duplicate ID is another occurrence"
    );
    click(&dom, mounted_element(&dom, "id", "file-pdf-close-1"));
    pending.send(Ok(metadata())).unwrap();
    pump(&mut dom, &mut attrs, || io.completed.get() == completed + 1).await;
    assert!(owned_pdf::display(&state.peek(), 1).is_none());
    assert_eq!(
        io.calls.borrow().len(),
        n + 1,
        "retired metadata cannot trigger IP/body"
    );
    prepare(
        &mut dom,
        &mut attrs,
        state,
        &io,
        "file-pdf-preview-0",
        true,
        Some(document.clone()),
    )
    .await;
    let Display::Document { id, nonce, .. } = owned_pdf::display(&state.peek(), 0).unwrap() else {
        panic!()
    };
    let stale_decoder = owned_pdf::Source::capture(&state.peek(), 0).unwrap();
    let team = state.peek().core.active_team.clone().unwrap();
    crate::file_events::invalidate(&mut state.write(), &team, "F12345678");
    assert!(
        owned_pdf::bytes(&state.peek(), &nonce, id.opaque()).is_none(),
        "confirmed file change revokes staging before tick"
    );
    assert!(!owned_pdf::decoded(state, stale_decoder, 3, false));
    assert!(pages::poll(&mut state.write()));
    assert_eq!(
        pages::rows(&state.peek()).len(),
        2,
        "event retirement does not mutate detached metadata"
    );
    original_page(&mut dom, &mut attrs, state, &pages_io).await;
    let n = io.calls.borrow().len();
    let before = io.urls.borrow().len();
    let done = io.body_completed.get();
    let m = io.expect(false);
    let ip = io.expect(true);
    let body = io.expect_bytes();
    click(&dom, mounted_element(&dom, "id", "file-pdf-preview-0"));
    pump(&mut dom, &mut attrs, || io.calls.borrow().len() == n + 1).await;
    m.send(Ok(metadata())).unwrap();
    pump(&mut dom, &mut attrs, || io.calls.borrow().len() == n + 2).await;
    ip.send(Ok(json!({"ok":true,"ip_allowed":true}))).unwrap();
    pump(&mut dom, &mut attrs, || {
        io.urls.borrow().len() == before + 1
    })
    .await;
    state.write().core.account_epoch += 1;
    body.send(Ok(document)).unwrap();
    pump(&mut dom, &mut attrs, || io.body_completed.get() == done + 1).await;
    assert!(owned_pdf::display(&state.peek(), 0).is_none());
    assert!(owned_pdf::bytes(&state.peek(), &nonce, id.opaque()).is_none());
    assert!(state.peek().viewer.is_none());
    assert_eq!(io.calls.borrow().len(), n + 2);
}
