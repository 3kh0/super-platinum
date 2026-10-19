//! Removal of a known reply from an acquired page's typed projection.
use super::Page;

impl Page {
    /// Remove only a known reply belonging to this channel and acquired root.
    ///
    /// The original HTTP `raw()` receipt, target and pagination remain unchanged.
    /// This method does not retain deletion evidence across replacement pages;
    /// the owner of the reader must fence later reads against newer deletions.
    pub fn remove_reply(&mut self, channel: &str, ts: &str) -> bool {
        let Some(root) = self.target.root_ts() else {
            return false;
        };
        if channel != self.target.channel_id() || ts.is_empty() || ts == root {
            return false;
        }
        let Some(index) = self.messages.iter().position(|message| {
            message.ts.as_deref() == Some(ts)
                && message.thread_ts.as_deref() == Some(root)
                && message
                    .channel
                    .as_deref()
                    .is_none_or(|value| value == channel)
        }) else {
            return false;
        };
        self.messages.remove(index);
        if let Some(root_index) = self.root_index.as_mut()
            && index < *root_index
        {
            *root_index -= 1;
        }
        true
    }
}

#[cfg(test)]
mod tests;
