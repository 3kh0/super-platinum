//! Future singleton responses only; the actual read retains its own source owner.
use super::*;
pub(crate) fn seed(shell: &ShellState) -> Result<Value, Error> {
    if !completion::empty_canvas::named() {
        return Err(refused());
    }
    let model = model(shell)?;
    let future = [CANVAS, TARGET_LIST]
        .into_iter()
        .map(|id| {
            let text = original_text(id)?;
            Ok((
                id.to_owned(),
                if id == CANVAS {
                    completion::empty_canvas::response(
                        text,
                        completion::empty_canvas::plain_seed(),
                    )?
                } else {
                    text.into()
                },
            ))
        })
        .collect::<Result<BTreeMap<_, _>, Error>>()?;
    let mut l = model.0.lock().map_err(|_| refused())?;
    if l.cold_files.is_some() {
        return Err(refused());
    }
    let proof = json!({"initialResponseTexts":future,"initialResponses":responses(&future),"canonicalSeeded":false});
    l.cold_files = Some(State {
        future,
        ..Default::default()
    });
    Ok(proof)
}
pub(crate) fn plain_and_invalidate(shell: &mut ShellState) -> Result<Value, Error> {
    if !completion::empty_canvas::named() {
        return Err(refused());
    }
    let model = model(shell)?;
    let text = completion::empty_canvas::response(original_text(CANVAS)?, true)?;
    let previous = {
        let mut l = model.0.lock().map_err(|_| refused())?;
        l.cold_files
            .as_mut()
            .ok_or_else(refused)?
            .future
            .insert(CANVAS.into(), text.clone())
    };
    let invalidation = completion::files::invalidate(shell, CANVAS)?;
    let proof = json!({"kind":"future-plain-response-and-actual-local-invalidation","id":CANVAS,"previousResponseText":previous,"futureResponseText":text,"futureResponse":parsed(&text)?,"invalidation":invalidation});
    model
        .0
        .lock()
        .map_err(|_| refused())?
        .cold_files
        .as_mut()
        .ok_or_else(refused)?
        .invalidations
        .push(proof.clone());
    Ok(proof)
}
