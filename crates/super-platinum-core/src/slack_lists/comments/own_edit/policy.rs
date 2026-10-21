use super::*;

/// Positive current observations supplied by the host's original Authority and
/// preference reads. No absent value defaults to unlimited or rich mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnEditPolicy {
    workspace: String,
    user: String,
    channel: Value,
    can_thread: bool,
    archive_view: bool,
    windows: Vec<i64>,
    plain: bool,
    portable: bool,
}
impl OwnEditPolicy {
    #[allow(clippy::too_many_arguments)]
    pub fn observed(
        workspace: &str,
        user: &str,
        channel: Value,
        can_thread: bool,
        archive_view: bool,
        edit_windows: Vec<i64>,
        plain_text_mode: bool,
        portable_content_composer: bool,
    ) -> Result<Self, ListError> {
        if !identity(workspace)
            || !identity(user)
            || !channel["id"].as_str().is_some_and(identity)
            || edit_windows.is_empty()
            || edit_windows.len() > 256
            || edit_windows
                .iter()
                .any(|v| !(-1..=9_007_199_254_740_991).contains(v))
        {
            return Err(ListError("The comment edit policy is unavailable."));
        }
        Ok(Self {
            workspace: workspace.into(),
            user: user.into(),
            channel,
            can_thread,
            archive_view,
            windows: edit_windows,
            plain: plain_text_mode,
            portable: portable_content_composer,
        })
    }
    pub fn channel_raw(&self) -> &Value {
        &self.channel
    }
    pub(super) fn admit(
        &self,
        workspace: &str,
        user: &str,
        target: &Target,
        ts: &str,
        now_ms: u64,
    ) -> Result<(), ListError> {
        target.validate()?;
        // JszJ: any -1 wins, otherwise the largest actual relevant window.
        let minutes = if self.windows.contains(&-1) {
            -1
        } else {
            *self.windows.iter().max().unwrap()
        };
        let sent = ts
            .parse::<f64>()
            .map_err(|_| ListError("The comment timestamp is malformed."))?;
        // nL6P uses milliseconds at >=1e10, then Date TimeClip truncation.
        let sent_ms = (if sent >= 1e10 { sent } else { sent * 1000.0 }).trunc();
        if self.workspace != workspace
            || self.user != user
            || self.channel["id"].as_str() != Some(target.channel_id())
            || self.plain
            || self.portable
            || self.archive_view
            || !self.can_thread
            || ["is_frozen", "is_archived", "is_non_threadable", "is_locked"]
                .iter()
                .any(|k| seed::truthy(self.channel.get(*k)))
            || (seed::truthy(self.channel.get("is_pending_ext_shared"))
                && seed::truthy(self.channel.get("is_im")))
            || minutes == 0
            || (minutes > -1 && (now_ms as f64 - sent_ms) / 60_000.0 > minutes as f64)
        {
            return Err(ListError(
                "The comment edit policy or edit window does not allow this edit.",
            ));
        }
        Ok(())
    }
}
