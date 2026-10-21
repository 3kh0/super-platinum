//! Renderer-local original callback witness; never reconstructed from DOM IDs.
use super::*;
use crate::canvas::comments::file_source::{self, Token};
thread_local! {
    static MODEL: std::cell::RefCell<Option<Signal<State>>> = const { std::cell::RefCell::new(None) };
    static ORIGINAL: std::cell::RefCell<Option<(Signal<State>,Token)>> = const { std::cell::RefCell::new(None) };
}
pub(in crate::canvas) fn remember_model(model: Signal<State>) {
    if requested() {
        MODEL.with(|slot| *slot.borrow_mut() = Some(model));
    }
}
pub(in crate::canvas) fn remember_token(shell: &ShellState) -> Result<(), String> {
    if !enabled(shell) {
        return Err("Original callback fixture retired".into());
    }
    let model = MODEL
        .with(|slot| *slot.borrow())
        .ok_or("Original model missing")?;
    let token = {
        let value = model.read();
        let message = value
            .rows
            .iter()
            .find(|row| row.message.ts.as_deref() == Some(ROOT))
            .map(|row| &row.message)
            .ok_or("Original row missing")?;
        file_source::token(shell, &value, message).ok_or("Original token unavailable")?
    };
    ORIGINAL.with(|slot| *slot.borrow_mut() = Some((model, token)));
    Ok(())
}
pub(in crate::canvas) fn invoke_original(state: Signal<ShellState>) -> Result<Value, String> {
    if !enabled(&state.peek()) {
        return Err("Original callback fixture retired".into());
    }
    let (model, token) = ORIGINAL
        .with(|slot| slot.borrow().clone())
        .ok_or("Original token missing")?;
    let current = token.current(&state.peek(), &model.read());
    let before = resolver::snapshot();
    crate::canvas::comments::file_navigation::open(state, model, token);
    let after = resolver::snapshot();
    if current || before != after {
        return Err("Retired original callback was admitted".into());
    }
    Ok(json!({"originalCurrent":current,"resolverUnchanged":before==after}))
}
