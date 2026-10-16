use super::*;
pub(super) fn confirm(mut state: Signal<ShellState>, serial: u64, visit: u64, structure: u64) {
    // peek deliberately avoids a disabling render before reading native input.
    let Some(capture) = filter::reserve(&state.peek(), serial, visit, structure) else {
        return;
    };
    let proof = capture.proof();
    spawn(async move {
        let mut eval = dioxus::document::eval(&format!(
            "{};\nlet owned;try{{owned=compoundFilterSnapshot({proof});dioxus.send(owned?.snapshot??null);const restore=await dioxus.recv();owned?.finish(restore===true);}}finally{{try{{owned?.finish(true);}}catch(_){{}}}}",
            include_str!("capture.js")
        ));
        let reply = tokio::time::timeout(
            capture.remaining(),
            eval.recv::<Option<Vec<filter::Snapshot>>>(),
        )
        .await;
        let submitted = if let Ok(Ok(Some(snapshot))) = reply {
            filter::submit(state, &capture, snapshot)
        } else {
            false
        };
        let restore = if submitted {
            false
        } else {
            filter::release(&state.peek(), &capture)
        };
        let _ = eval.send(restore);
        if restore {
            state.write();
        }
        // Drop also releases a reservation if this task is canceled at any await.
        drop(capture);
    });
}
