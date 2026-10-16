pub(super) fn available(shell: &ShellState) -> bool {
    shell.core.transport.is_some()
        && shell.core.active_team.as_ref().is_some_and(|team| {
            shell.core.workspaces.contains_key(team)
                && shell
                    .core
                    .session
                    .as_ref()
                    .is_some_and(|s| s.workspaces.contains_key(team))
        })
}
use super::*;
pub(super) struct Scope {
    pub context: crate::scheduled::Context,
    cookie: String,
    account: Option<String>,
    socket: u64,
}
impl Scope {
    pub fn capture(shell: &ShellState) -> Option<Self> {
        let context = crate::scheduled::Context::capture(shell)?;
        let socket = shell
            .core
            .workspaces
            .get(&context.session.team_id)?
            .rt_generation;
        Some(Self {
            context,
            cookie: shell.core.session.as_ref()?.d_cookie.clone(),
            account: shell.core.active_account.clone(),
            socket,
        })
    }
    pub fn current(&self, shell: &ShellState) -> bool {
        self.context.current(shell)
            && shell.core.active_account == self.account
            && shell.core.session.as_ref().is_some_and(|s| {
                s.d_cookie == self.cookie
                    && s.workspaces
                        .get(&self.context.session.team_id)
                        .is_some_and(|w| {
                            w.team_id == self.context.session.team_id
                                && w.user_id == self.context.session.user_id
                                && w.token == self.context.session.token
                                && w.url == self.context.session.url
                                && w.enterprise_id == self.context.session.enterprise_id
                        })
            })
            && shell
                .core
                .workspaces
                .get(&self.context.session.team_id)
                .is_some_and(|w| w.rt_generation == self.socket)
    }
}
pub(super) fn text_mime(mime: &str) -> bool {
    // Match FileViewer's Text branch, not merely the XML suffix decoder helper.
    !mime.starts_with("image/")
        && !mime.starts_with("audio/")
        && !mime.starts_with("video/")
        && mime != "application/pdf"
        && file_preview::is_text(mime)
}
pub(super) fn hosted(raw: &str) -> bool {
    url::Url::parse(raw).is_ok_and(|u| {
        u.scheme() == "https"
            && u.username().is_empty()
            && u.password().is_none()
            && u.port_or_known_default() == Some(443)
            && u.fragment().is_none()
            && u.host_str().is_some_and(|h| {
                h == "slack.com"
                    || h.ends_with(".slack.com")
                    || h == "slack-edge.com"
                    || h.ends_with(".slack-edge.com")
            })
    })
}
pub(super) struct Source {
    pub url: String,
    pub mime: String,
}
impl Source {
    pub fn parse(file: &File, id: &str) -> Option<Self> {
        if file.id.as_deref() != Some(id)
            || file_actions::deleted(file)
            || !file_actions::renameable(file)
            || file.is_external != Some(false)
            || !matches!(
                file.extra.get("mode").and_then(|v| v.as_str()),
                Some("hosted" | "snippet")
            )
        {
            return None;
        }
        // Require fields from this exact read; don't manufacture a source from old metadata.
        let mime = file.mimetype.as_deref()?.trim();
        if !text_mime(mime) {
            return None;
        }
        let url = super_platinum_core::state::file_download_url(file)?;
        if !hosted(url) {
            return None;
        }
        Some(Self {
            url: url.into(),
            mime: mime.into(),
        })
    }
}
