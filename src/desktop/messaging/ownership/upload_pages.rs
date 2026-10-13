//! Bounded read-only receipt lookup. A partial scan never establishes rejection.
use super::*;
use super_platinum_core::slack::models::HistoryPage;

const MAX_PAGES: usize = 10;

pub(in crate::messaging) async fn find<F, Fut>(
    expected: &UploadReadback,
    mut fetch: F,
    current: impl Fn() -> bool,
) -> Result<HistoryPage, Error>
where
    F: FnMut(Option<String>) -> Fut,
    Fut: std::future::Future<Output = Result<HistoryPage, Error>>,
{
    let mut cursor = None;
    let mut seen = std::collections::HashSet::new();
    for _ in 0..MAX_PAGES {
        if !current() {
            return Err(Error::Transport("Upload readback owner changed".into()));
        }
        let page = fetch(cursor).await?;
        if !current() {
            return Err(Error::Transport("Upload readback owner changed".into()));
        }
        if page
            .messages
            .iter()
            .any(|message| expected.confirms(message))
        {
            return Ok(page);
        }
        let next = page
            .response_metadata
            .as_ref()
            .and_then(|metadata| metadata.next_cursor.as_ref())
            .filter(|cursor| !cursor.trim().is_empty())
            .cloned();
        let Some(next) = next else {
            return if page.has_more {
                Err(Error::Transport(
                    "Upload readback page omitted its continuation".into(),
                ))
            } else {
                Ok(HistoryPage::default())
            };
        };
        if !seen.insert(next.clone()) {
            return Err(Error::Transport(
                "Upload readback repeated its continuation".into(),
            ));
        }
        cursor = Some(next);
    }
    Err(Error::Transport(
        "Upload readback reached its page budget".into(),
    ))
}
#[cfg(test)]
mod tests;
