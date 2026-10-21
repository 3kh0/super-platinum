//! Original-prefix routing (8Bbq) and remote/local normalization (hHTo).
use super::Kind;
use super::protocol;
use protocol::settings_acquisition::SearchKind;
pub(super) struct Query<'a> {
    raw: &'a str,
    remote: &'a str,
    local: &'a str,
}
impl<'a> Query<'a> {
    pub(super) fn new(raw: &'a str) -> Self {
        let remote = raw
            .strip_prefix(['@', '\u{ff20}', '#', '\u{ff03}', '/', '\u{ff0f}'])
            .unwrap_or(raw);
        let local = remote
            .strip_prefix([
                '@', '\u{ff20}', '#', '\u{ff03}', '/', '\u{ff0f}', '+', '\u{ff0b}',
            ])
            .unwrap_or(remote);
        Self { raw, remote, local }
    }
    pub(super) fn remote(&self) -> &str {
        self.remote
    }
    pub(super) fn local(&self) -> &str {
        self.local
    }
    pub(super) fn accepts(&self, kind: Kind, external: bool) -> bool {
        if external {
            return matches!(kind, Kind::Team | Kind::Org);
        }
        match self.raw.chars().next() {
            Some('@' | '\u{ff20}') => kind == Kind::User,
            Some('#' | '\u{ff03}') => kind == Kind::Channel,
            _ => true,
        }
    }
    pub(super) fn routes(&self, external: bool, enterprise: bool) -> Vec<SearchKind> {
        if external {
            return vec![SearchKind::ExternalTeams];
        }
        match self.raw.chars().next() {
            Some('@' | '\u{ff20}') => vec![SearchKind::Members],
            Some('#' | '\u{ff03}') => vec![SearchKind::Channels],
            _ => {
                let mut kinds = vec![SearchKind::Members, SearchKind::Channels];
                if enterprise {
                    kinds.push(SearchKind::InternalTeams);
                }
                kinds
            }
        }
    }
}
pub(super) fn more(kind: SearchKind, rows: usize, marker: Option<&str>) -> bool {
    marker.is_some()
        && match kind {
            SearchKind::InternalTeams => false,
            SearchKind::Members | SearchKind::Channels => rows < 24,
            _ => true,
        }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_prefix_selects_route_before_remote_normalization() {
        for p in ["@", "\u{ff20}"] {
            let s = format!("{p}ada");
            let q = Query::new(&s);
            assert_eq!(q.remote(), "ada");
            assert_eq!(q.routes(false, true), vec![SearchKind::Members]);
            assert!(!q.accepts(Kind::Channel, false));
        }
        for p in ["#", "\u{ff03}"] {
            let s = format!("{p}run");
            let q = Query::new(&s);
            assert_eq!(q.remote(), "run");
            assert_eq!(q.routes(false, true), vec![SearchKind::Channels]);
        }
        let q = Query::new("/@ada");
        assert_eq!(q.remote(), "@ada");
        assert_eq!(q.local(), "ada");
        assert_eq!(q.routes(false, true).len(), 3);
        assert_eq!(Query::new("+ada").remote(), "+ada");
        assert_eq!(Query::new("+ada").local(), "ada");
    }
    #[test]
    fn pagination_follows_actual_count_and_route() {
        for k in [SearchKind::Members, SearchKind::Channels] {
            assert!(more(k, 23, Some("next")));
            assert!(!more(k, 24, Some("next")));
            assert!(!more(k, 1, None));
        }
        assert!(!more(SearchKind::InternalTeams, 1, Some("next")));
        for k in [SearchKind::ExternalTeams, SearchKind::AvailableExternal] {
            assert!(more(k, 100, Some("next")));
            assert!(!more(k, 0, None));
        }
    }
}
