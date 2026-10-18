//! g5os.Nt/U6KP/TckT: resolve an app through Slack before choosing its tab.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Default,
    Home,
    Messages,
    About,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub app: String,
    pub installed_team: String,
    pub requested: Tab,
}
/// Parsed URI workspace and app installation remain separate from authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub workspace: String,
    pub target: Target,
}
impl Link {
    /// Captured parsed Slack URI command `app`: id, optional team and tab.
    /// This parses a destination only. The desktop must resolve the exact team
    /// in the current account before switching or building a request.
    pub fn parse(link: &str, current_team: &str) -> Option<Self> {
        if link.len() > 8192 {
            return None;
        }
        let url = url::Url::parse(link).ok()?;
        if url.scheme() != "slack"
            || url.host_str() != Some("app")
            || !url.path().is_empty()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.port().is_some()
            || url.fragment().is_some()
        {
            return None;
        }
        let mut app = None;
        let mut team = None;
        let mut tab = None;
        for (key, value) in url.query_pairs() {
            let slot = match key.as_ref() {
                "id" => &mut app,
                "team" => &mut team,
                "tab" => &mut tab,
                _ => continue,
            };
            if slot.replace(value.into_owned()).is_some() {
                return None;
            }
        }
        let team = team.unwrap_or_else(|| current_team.into());
        let requested = match tab.as_deref() {
            None => Tab::Default,
            Some("home" | "app") => Tab::Home,
            Some("messages") => Tab::Messages,
            Some("about") => Tab::About,
            _ => return None,
        };
        Some(Self {
            workspace: team.clone(),
            target: Target::new(app?, team, requested).ok()?,
        })
    }
}

impl Target {
    /// Compatibility entry for callers that own only the current workspace.
    pub fn from_link(link: &str, current_team: &str) -> Option<Self> {
        let link = Link::parse(link, current_team)?;
        (link.workspace == current_team).then_some(link.target)
    }
    pub fn new(app: String, installed_team: String, requested: Tab) -> Result<Self, Error> {
        if !id(&app) || !id(&installed_team) {
            return Err(invalid());
        }
        Ok(Self {
            app,
            installed_team,
            requested,
        })
    }
    pub fn prepare(
        &self,
        client: &SlackClient,
        w: &WorkspaceSession,
    ) -> Result<PreparedRequest, Error> {
        Self::new(
            self.app.clone(),
            self.installed_team.clone(),
            self.requested,
        )?;
        Ok(client.rest_form(
            w,
            "apps.home.open",
            vec![
                ("app_id", self.app.clone()),
                ("app_installed_team_id", self.installed_team.clone()),
                ("_x_reason", "app-home-open-api".into()),
            ],
        ))
    }
    pub fn decode(&self, raw: &Value) -> Result<(Home, Tab), Error> {
        let channel = text(raw.get("app_home").ok_or_else(invalid)?, "conversation_id")?;
        let (home, _) = Home::decode(&channel, raw)?;
        if home.app != self.app || home.installed_team != self.installed_team {
            return Err(invalid());
        }
        let tab = choose_tab(self.requested, home.messages, home.enabled);
        Ok((home, tab))
    }
}

/// Exact enabled-tab fallback from TckT.k for these three explicit requests.
/// About is its own surface when both conversational tabs are unavailable.
pub fn choose_tab(requested: Tab, messages: bool, home: bool) -> Tab {
    match requested {
        Tab::Default => choose_tab(Tab::Messages, messages, home),
        Tab::Messages if !messages => {
            if home {
                Tab::Home
            } else {
                Tab::About
            }
        }
        Tab::Home if !home => {
            if messages {
                Tab::Messages
            } else {
                Tab::About
            }
        }
        tab => tab,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn app_open_uses_captured_endpoint_and_metadata_reason_with_exact_form() {
        use crate::slack::client::RequestBody;
        let w = WorkspaceSession {
            team_id: "T1".into(),
            user_id: "U1".into(),
            enterprise_id: None,
            name: "Synthetic".into(),
            url: "https://example.test".into(),
            token: "synthetic".into(),
        };
        let request = Target::new("A1".into(), "T_INSTALL".into(), Tab::Home)
            .unwrap()
            .prepare(&SlackClient::default(), &w)
            .unwrap();
        assert_eq!(request.method, "POST");
        let url = url::Url::parse(&request.url).unwrap();
        assert_eq!(url.origin().ascii_serialization(), "https://example.test");
        assert_eq!(url.path(), "/api/apps.home.open");
        assert!(url.fragment().is_none());
        // SlackClient adds captured client metadata after the exact endpoint path.
        let keys: Vec<_> = url.query_pairs().map(|(key, _)| key.into_owned()).collect();
        assert_eq!(
            keys,
            [
                "_x_id",
                "_x_csid",
                "_x_desktop_ia",
                "_x_foreground",
                "_x_frontend_build_type",
                "_x_gantry",
                "_x_num_retries",
                "_x_version_ts",
                "fp",
                "slack_route",
            ]
        );
        let RequestBody::Form(fields) = request.body else {
            panic!("form")
        };
        assert_eq!(fields.len(), 4);
        let fields: std::collections::BTreeMap<_, _> = fields.into_iter().collect();
        assert_eq!(fields.len(), 4);
        assert_eq!(fields["app_id"], "A1");
        assert_eq!(fields["app_installed_team_id"], "T_INSTALL");
        assert_eq!(fields["_x_reason"], "app-home-open-api");
        assert_eq!(fields["token"], "synthetic");
    }
    #[test]
    fn exact_enabled_tab_matrix_preserves_about_instead_of_disabled_messages() {
        for (messages, home, expected_messages, expected_home) in [
            (true, true, Tab::Messages, Tab::Home),
            (true, false, Tab::Messages, Tab::Messages),
            (false, true, Tab::Home, Tab::Home),
            (false, false, Tab::About, Tab::About),
        ] {
            assert_eq!(choose_tab(Tab::Messages, messages, home), expected_messages);
            assert_eq!(choose_tab(Tab::Default, messages, home), expected_messages);
            assert_eq!(choose_tab(Tab::Home, messages, home), expected_home);
            assert_eq!(choose_tab(Tab::About, messages, home), Tab::About);
        }
    }
    #[test]
    fn app_uri_preserves_explicit_team_and_tab_without_guessing_cross_workspace_authority() {
        let target = Target::from_link("slack://app?team=T1&id=A1&tab=home", "T1").unwrap();
        assert_eq!(
            target,
            Target::new("A1".into(), "T1".into(), Tab::Home).unwrap()
        );
        assert_eq!(
            Target::from_link("slack://app?id=A1", "T1")
                .unwrap()
                .requested,
            Tab::Default
        );
        for link in [
            "slack://app?team=T2&id=A1",
            "slack://app?id=A1&id=A2",
            "slack://app?id=A1&tab=unknown",
            "slack://channel?id=A1",
            "slack://app/path?id=A1",
            "https://example.test/app?id=A1",
        ] {
            assert!(Target::from_link(link, "T1").is_none());
        }
    }
    #[test]
    fn returned_conversation_is_authoritative_but_app_and_installation_must_match() {
        let target = Target::new("A_CAL".into(), "T_INSTALL".into(), Tab::Home).unwrap();
        let raw = json!({"ok":true,"app_home":{"conversation_id":"D_RETURNED",
            "app_id":"A_CAL","app_installed_team_id":"T_INSTALL",
            "home_tab_enabled":true,"messages_tab_enabled":true}});
        let (home, tab) = target.decode(&raw).unwrap();
        assert_eq!(home.channel, "D_RETURNED");
        assert_eq!(tab, Tab::Home);
        for (key, value) in [
            ("app_id", json!("A_OTHER")),
            ("app_installed_team_id", json!("T_OTHER")),
            ("conversation_id", json!(null)),
            ("home_tab_enabled", json!("true")),
        ] {
            let mut changed = raw.clone();
            changed["app_home"][key] = value;
            assert!(target.decode(&changed).is_err());
        }
    }
}

#[cfg(test)]
mod link_tests {
    use super::*;
    #[test]
    fn uri_workspace_is_a_destination_not_an_authorization_or_bot_mapping() {
        let link = Link::parse("slack://app?team=T_OTHER&id=A1&tab=messages", "T_CURRENT").unwrap();
        assert_eq!(link.workspace, "T_OTHER");
        assert_eq!(link.target.installed_team, "T_OTHER");
        assert_eq!(link.target.app, "A1");
        assert_eq!(link.target.requested, Tab::Messages);
        assert!(Target::from_link("slack://app?team=T_OTHER&id=A1", "T_CURRENT").is_none());
        assert_eq!(
            Link::parse("slack://app?id=A1", "T_CURRENT")
                .unwrap()
                .workspace,
            "T_CURRENT"
        );
        for raw in [
            "slack://app?team=T1&team=T2&id=A1",
            "slack://app?team=&id=A1",
            "slack://app?team=T1&id=A1#other",
            "slack://app?team=T1&id=A1&tab=unknown",
        ] {
            assert!(Link::parse(raw, "T_CURRENT").is_none());
        }
    }
}
