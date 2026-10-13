//! Thread viewport sampling mirrors channel windowing and shares its DOM anchor.
use crate::state::ShellState;
use crate::thread_window::PaintPiece;
use dioxus::prelude::*;

pub(super) fn rows(
    state: Signal<ShellState>,
    shell: &ShellState,
    channel: &str,
    history: bool,
) -> Element {
    // Handwritten small fixtures predate the lightweight thread geometry.
    if shell.thread_window.rows.is_empty() {
        return rsx! {for message in shell.thread_messages.iter().filter(|m|shell.thread_history.contains(shell.thread_root.as_deref().unwrap_or_default(),&m.ts)==history) {
            div {key:"thread-{message.id}","data-row-id":"{message.id}",
                if message.show_unread_divider {div {class:"unread-divider",span {"New"}}}
                {super::message::message_row(state,shell,message,channel,None,super::message::RowSurface::Thread,shell.thread_root.clone().map(|root|crate::message_buttons::source::Bag::Thread{root}))}
            }
        }};
    }
    let valid = shell.thread_messages.len() == shell.thread_window.selected_indices().len();
    rsx! {for piece in shell.thread_window.pieces(history) {match *piece {
        PaintPiece::Spacer{index,height}=>rsx! {div {key:"thread-spacer-{index}","data-thread-spacer":true,style:"height:{height}px;flex-shrink:0","aria-hidden":true}},
        PaintPiece::Row{index,projected}=> {
            let message = valid.then(|| shell.thread_messages.get(projected)).flatten().filter(|message| shell.thread_window.rows[index].ts == message.ts);
            if let Some(message)=message {
                rsx! {div {key:"thread-{message.id}","data-row-id":"{message.id}","data-thread-index":index,
                    if message.show_unread_divider {div {class:"unread-divider",span {"New"}}}
                    {super::message::message_row(state,shell,message,channel,None,super::message::RowSurface::Thread,shell.thread_root.clone().map(|root|crate::message_buttons::source::Bag::Thread{root}))}
                }}
            } else {
                let height=shell.thread_window.height(&shell.thread_window.rows[index].ts);
                rsx! {div {key:"thread-missing-{index}","data-thread-spacer":true,style:"height:{height}px;flex-shrink:0","aria-hidden":true}}
            }
        }
    }}}
}

pub(super) fn schedule(
    state: Signal<ShellState>,
    mut pending: Signal<bool>,
    mut queued: Signal<bool>,
) {
    if *pending.peek() {
        queued.set(true);
        return;
    }
    pending.set(true);
    queued.set(false);
    spawn(async move {
        measure(state, queued).await;
        pending.set(false);
        if *queued.peek() {
            schedule(state, pending, queued);
        }
    });
}
async fn measure(mut state: Signal<ShellState>, mut queued: Signal<bool>) {
    let scope = state.peek().thread_scope();
    let input_generation = state.peek().thread_window.input_generation;
    let projection_generation = state.peek().thread_window.projection_generation;
    let mut eval = dioxus::document::eval(
        r#"await (async()=>{
      await new Promise(requestAnimationFrame);
      const body=document.getElementById('thread-body');
      if(!body){dioxus.send(null);await dioxus.recv();return;}
      const bounds=body.getBoundingClientRect();
      const rows=[...body.querySelectorAll('[data-thread-index]')].map(row=>{
        const r=row.getBoundingClientRect();return {index:Number(row.dataset.threadIndex),id:row.dataset.rowId,height:r.height,top:r.top,bottom:r.bottom};
      });
      const visible=rows.filter(r=>r.bottom>bounds.top&&r.top<bounds.bottom);
      const earlier=body.querySelector('[data-thread-page="earlier"]');
      const newer=body.querySelector('[data-thread-page="newer"]');
      const extent=element=>{if(!element)return 0;const style=getComputedStyle(element);return element.getBoundingClientRect().height+parseFloat(style.marginTop||0)+parseFloat(style.marginBottom||0);};
      dioxus.send({scope:body.dataset.timelineScope,projectionGeneration:body.dataset.threadProjectionGeneration,first:visible[0],last:visible[visible.length-1],
        top:Math.max(0,body.scrollTop-8-extent(earlier)),height:body.clientHeight,
        gap:extent(newer),rows:rows.map(r=>[r.id,r.height]),
        atBottom:body.scrollHeight-body.scrollTop-body.clientHeight<48});
      await dioxus.recv();
    })();"#,
    );
    let receipt = eval.recv::<serde_json::Value>().await;
    let _ = eval.send("received");
    let Ok(value) = receipt else { return };
    if value.is_null()
        || *queued.peek()
        || state.peek().thread_scope() != scope
        || value["scope"].as_str() != Some(&scope)
    {
        return;
    }
    // Require both the current canonical projection and its committed DOM.
    // A same-visit content edit can retain every timestamp but change heights.
    if !state.peek().thread_window.projection_receipt_is_current(
        projection_generation,
        value["projectionGeneration"].as_str(),
    ) {
        queued.set(true);
        return;
    }
    // A receipt sampled before input must not restore bottom intent after
    // that input retires it. The DOM scroll event may still be in transit.
    if !state.peek().thread_window.rows.is_empty()
        && !state
            .peek()
            .thread_window
            .measurement_is_current(&scope, input_generation)
    {
        queued.set(true);
        return;
    }
    if state.peek().thread_window.rows.is_empty() {
        let bottom = value["atBottom"].as_bool().unwrap_or(false);
        if state.peek().thread_target.is_none() && state.peek().thread_at_bottom != bottom {
            state.write().thread_at_bottom = bottom;
        }
        return;
    }
    let heights = value["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| Some((r[0].as_str()?.to_owned(), r[1].as_f64()?)))
        .collect::<Vec<_>>();
    let bottom = value["atBottom"].as_bool().unwrap_or(false);
    let (first, last) = {
        let shell = state.peek();
        let read = |name: &str| {
            let r = &value[name];
            let index = r["index"].as_u64()? as usize;
            (shell.thread_window.rows.get(index)?.ts == r["id"].as_str()?).then_some(index)
        };
        if !value["first"].is_null() {
            let (Some(first), Some(last)) = (read("first"), read("last")) else {
                return;
            };
            (first, last)
        } else {
            let gap_before = shell
                .thread_window
                .rows
                .iter()
                .position(|r| !r.history)
                .unwrap_or(shell.thread_window.rows.len());
            shell.thread_window.estimated(
                value["top"].as_f64().unwrap_or(0.0),
                value["height"].as_f64().unwrap_or(0.0),
                value["gap"].as_f64().unwrap_or(0.0),
                gap_before,
            )
        }
    };
    // On initial mount the browser can report the old top before the requested
    // bottom has painted. Keep the newest neighborhood until navigation lands.
    let awaiting_bottom =
        state.peek().thread_at_bottom && !state.peek().thread_window.measured && !bottom;
    if awaiting_bottom {
        return;
    }
    let target = state.peek().thread_target.is_some();
    let changed = {
        let shell = state.peek();
        shell
            .thread_window
            .would_change(first, last, &heights, target)
            || (!target && shell.thread_at_bottom != bottom)
    };
    if changed {
        let mut shell = state.write();
        shell.thread_window.measure(first, last, &heights, target);
        if !target {
            shell.thread_at_bottom = bottom;
        }
        shell.reproject_thread_window();
        crate::thread_unread::annotate(&mut shell);
    }
}
