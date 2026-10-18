//! Build the file bucket for the ONE shared Texty75/25 mixed rank invocation.
use super::file_matching::FileScored;
use crate::block_actions::home::workflow::directory_ranking::{Context, Error, files};
use serde_json::{Value, json};

pub fn rows(matched: &[FileScored<'_>], context: &Context<'_>) -> Result<Vec<Value>, Error> {
    matched
        .iter()
        .map(|m| {
            if m.id != m.snapshot.id() || m.score > 3 {
                return Err(Error::InvalidInput);
            }
            let item = files::item(m.snapshot.effective(), context)?;
            let mut row = json!({"id":m.id,"item":item,"score":m.score});
            if let Some(exact) = m.is_exact_match {
                row["isExactMatch"] = json!(exact);
            }
            Ok(row)
        })
        .collect()
}
