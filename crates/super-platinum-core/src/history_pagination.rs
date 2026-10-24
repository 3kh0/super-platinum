//! Transient ordinary-history continuation. A cursor never discards its time range.
use crate::{
    slack::{api::HistoryArgs, models::HistoryPage},
    state::cmp_ts,
};
use std::collections::BTreeSet;

#[derive(Clone, Debug)]
pub struct Pages {
    original: HistoryArgs,
    request: HistoryArgs,
    seen: BTreeSet<String>,
    minimum: Option<String>,
    terminal: bool,
    restarted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Invalid {
    PageSize,
    Timestamp,
    Range,
    RepeatedCursor,
    NoProgress,
    ExpiredAgain,
}

impl Pages {
    pub fn new(request: HistoryArgs) -> Self {
        Self {
            original: request.clone(),
            request,
            seen: BTreeSet::new(),
            minimum: None,
            terminal: false,
            restarted: false,
        }
    }
    pub fn request(&self) -> &HistoryArgs {
        &self.request
    }
    pub fn has_more(&self) -> bool {
        !self.terminal
    }
    pub fn seen(&self) -> &BTreeSet<String> {
        &self.seen
    }
    pub fn restarted(&self) -> bool {
        self.restarted
    }

    /// Validate before publication. Failure leaves the exact pre-request state intact.
    pub fn advance(&self, page: &HistoryPage) -> Result<Self, Invalid> {
        if self.terminal {
            return Err(Invalid::NoProgress);
        }
        if page.messages.len() > self.request.limit.unwrap_or(100) as usize {
            return Err(Invalid::PageSize);
        }
        let mut next = self.clone();
        let mut oldest: Option<&str> = None;
        for message in &page.messages {
            let ts = message
                .ts
                .as_deref()
                .filter(|ts| valid_ts(ts))
                .ok_or(Invalid::Timestamp)?;
            for (bound, lower) in [(&self.request.oldest, true), (&self.request.latest, false)] {
                if let Some(bound) = bound {
                    let order = cmp_ts(Some(ts), Some(bound));
                    if (lower && order.is_lt())
                        || (!lower && order.is_gt())
                        || (order.is_eq() && !self.request.inclusive)
                    {
                        return Err(Invalid::Range);
                    }
                }
            }
            if oldest.is_none_or(|prior| cmp_ts(Some(ts), Some(prior)).is_lt()) {
                oldest = Some(ts);
            }
        }
        if let Some(oldest) = oldest {
            if next
                .minimum
                .as_deref()
                .is_none_or(|prior| cmp_ts(Some(oldest), Some(prior)).is_lt())
            {
                next.minimum = Some(oldest.to_owned());
            }
        }
        let cursor = page
            .response_metadata
            .as_ref()
            .and_then(|m| m.next_cursor.as_deref())
            .filter(|s| !s.is_empty());
        if let Some(cursor) = cursor {
            if self.request.cursor.as_deref() == Some(cursor)
                || !next.seen.insert(cursor.to_owned())
            {
                return Err(Invalid::RepeatedCursor);
            }
            next.request.cursor = Some(cursor.to_owned());
        } else if page.has_more {
            let oldest = oldest.ok_or(Invalid::NoProgress)?;
            if self.request.latest.as_deref().is_some_and(|prior| {
                let order = cmp_ts(Some(oldest), Some(prior));
                order.is_gt() || (order.is_eq() && !self.request.inclusive)
            }) {
                return Err(Invalid::NoProgress);
            }
            next.request.cursor = None;
            next.request.latest = Some(oldest.to_owned());
            next.request.inclusive = false;
        } else {
            next.terminal = true;
            next.request.cursor = None;
        }
        Ok(next)
    }

    /// One bounded fresh-range restart after an explicit invalid_cursor response.
    /// Restarting the original range avoids assuming that a short cursor page
    /// exhausted every timestamp above its oldest row. Cached rows are upserted.
    pub fn expired(&self) -> Result<Self, Invalid> {
        if self.restarted || self.request.cursor.is_none() {
            return Err(Invalid::ExpiredAgain);
        }
        let mut next = Self::new(self.original.clone());
        next.request.cursor = None;
        next.restarted = true;
        Ok(next)
    }
}

fn valid_ts(ts: &str) -> bool {
    let Some((seconds, fraction)) = ts.split_once('.') else {
        return false;
    };
    !seconds.is_empty()
        && !fraction.is_empty()
        && fraction.len() <= 6
        && seconds.bytes().all(|b| b.is_ascii_digit())
        && fraction.bytes().all(|b| b.is_ascii_digit())
        && seconds.parse::<u64>().is_ok()
}

#[cfg(test)]
mod tests;
