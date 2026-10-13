//! Standalone native default selection. Tab/window-return rules are deliberately
//! not inferred. Caller must reject old account/list/visit async completions.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionOrigin {
    Explicit,
    Template,
    Metadata,
    AllItems,
    Legacy,
    FirstView,
    Resolved,
    Personal,
    Manual,
    Fallback,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefaultSelection {
    pub selected_id: Option<String>,
    pub origin: SelectionOrigin,
    explicit: bool,
    manual: bool,
    async_default_applied: bool,
}
impl DefaultSelection {
    pub fn new(
        document: &Document,
        explicit: Option<&str>,
        default_to_all_items: bool,
        legacy_pref: Option<&str>,
    ) -> Result<Self> {
        let views = catalog(document)?;
        if let Some(id) = explicit.filter(|id| !id.is_empty()) {
            validate_id(&views, id)?;
            return Ok(Self {
                selected_id: Some(id.into()),
                origin: SelectionOrigin::Explicit,
                explicit: true,
                manual: false,
                async_default_applied: false,
            });
        }
        let valid = |id: &str| {
            views
                .iter()
                .any(|view| view.id == id && view.kind != "record")
        };
        let candidate = views
            .iter()
            .find(|view| view.kind != "record" && view.is_template_initial_view)
            .map(|v| (v.id.clone(), SelectionOrigin::Template))
            .or_else(|| {
                document
                    .raw
                    .pointer("/list_metadata/default_view")
                    .and_then(Value::as_str)
                    .filter(|id| valid(id))
                    .map(|id| (id.into(), SelectionOrigin::Metadata))
            })
            .or_else(|| {
                default_to_all_items
                    .then(|| {
                        views.iter().find(|v| {
                            v.kind != "record"
                                && (v.is_all_items_view
                                    || v.default_view_key.as_deref() == Some("all_items"))
                        })
                    })
                    .flatten()
                    .map(|v| (v.id.clone(), SelectionOrigin::AllItems))
            })
            .or_else(|| {
                legacy_pref
                    .filter(|id| valid(id))
                    .map(|id| (id.into(), SelectionOrigin::Legacy))
            })
            .or_else(|| {
                views
                    .iter()
                    .find(|v| !matches!(v.kind.as_str(), "record" | "canvas_inline"))
                    .map(|v| (v.id.clone(), SelectionOrigin::FirstView))
            });
        let (selected_id, origin) = candidate
            .map(|(id, origin)| (Some(id), origin))
            .unwrap_or((None, SelectionOrigin::Fallback));
        Ok(Self {
            selected_id,
            origin,
            explicit: false,
            manual: false,
            async_default_applied: false,
        })
    }
    /// None means a deliberate selection of the separately named native fallback.
    pub fn choose(&mut self, document: &Document, id: Option<&str>) -> Result<()> {
        if let Some(id) = id {
            validate_id(&catalog(document)?, id)?;
        }
        self.selected_id = id.map(str::to_owned);
        self.origin = if id.is_some() {
            SelectionOrigin::Manual
        } else {
            SelectionOrigin::Fallback
        };
        self.manual = true;
        Ok(())
    }
    pub fn apply_resolved(&mut self, document: &Document, id: Option<&str>) -> Result<bool> {
        self.apply_async(document, id, SelectionOrigin::Resolved)
    }
    /// Mirrors the captured personal effect: any present resolved default
    /// suppresses it, and both effects share a one-time actual-change guard.
    pub fn apply_personal(
        &mut self,
        document: &Document,
        id: Option<&str>,
        resolved_present: bool,
    ) -> Result<bool> {
        if resolved_present {
            return Ok(false);
        }
        self.apply_async(document, id, SelectionOrigin::Personal)
    }
    fn apply_async(
        &mut self,
        document: &Document,
        id: Option<&str>,
        origin: SelectionOrigin,
    ) -> Result<bool> {
        if self.explicit || self.manual || self.async_default_applied {
            return Ok(false);
        }
        let Some(id) = id.filter(|id| !id.is_empty()) else {
            return Ok(false);
        };
        validate_id(&catalog(document)?, id)?;
        if self.selected_id.as_deref() == Some(id) {
            return Ok(false);
        }
        self.selected_id = Some(id.into());
        self.origin = origin;
        self.async_default_applied = true;
        Ok(true)
    }
    /// A removed selection stays explicit to its caller, which can show the
    /// reason and offer fallback; it is never silently relabeled as another view.
    pub fn reconcile(&self, document: &Document) -> Result<()> {
        if let Some(id) = self.selected_id.as_deref() {
            validate_id(&catalog(document)?, id)?;
        }
        Ok(())
    }
}
fn validate_id(views: &[SavedView], id: &str) -> Result<()> {
    if views
        .iter()
        .any(|view| view.id == id && view.kind != "record")
    {
        Ok(())
    } else {
        Err(unsupported(
            "The selected saved view is missing or is a record layout.",
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompletionPolicy {
    pub apply_rules: bool,
    pub effective_show_completed: bool,
}
impl CompletionPolicy {
    /// All Option<bool> policy inputs mean verified value or unavailable evidence.
    /// A saved/local value can be absent by schema; saved absence defaults false.
    pub fn resolve(
        todo_mode: bool,
        grid_todos: Option<bool>,
        personal: Option<bool>,
        rollback: Option<bool>,
        saved: Option<bool>,
        local: Option<bool>,
    ) -> Result<Self> {
        let show = local.or(saved).unwrap_or(false);
        if !todo_mode {
            return Ok(Self {
                apply_rules: false,
                effective_show_completed: show,
            });
        }
        let grid = grid_todos.ok_or_else(|| {
            unsupported("Task views require a verified lists_grid_todos assignment.")
        })?;
        if !grid {
            return Ok(Self {
                apply_rules: false,
                effective_show_completed: show,
            });
        }
        let personal = personal.ok_or_else(|| {
            unsupported("Task views require a verified personal-task List policy.")
        })?;
        let show = if personal {
            let rollback = rollback.ok_or_else(|| {
                unsupported(
                    "Personal task views require a verified lists_todos_rollback assignment.",
                )
            })?;
            if !rollback { true } else { show }
        } else {
            show
        };
        Ok(Self {
            apply_rules: true,
            effective_show_completed: show,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn personal_and_resolved_defaults_share_settlement_and_manual_guard() {
        let doc = document();
        let mut state = DefaultSelection::new(&doc, None, false, None).unwrap();
        assert!(!state.apply_personal(&doc, Some("V1"), true).unwrap());
        assert_eq!(state.selected_id.as_deref(), Some("V2"));
        assert!(state.apply_personal(&doc, Some("V1"), false).unwrap());
        assert_eq!(state.origin, SelectionOrigin::Personal);
        assert!(!state.apply_resolved(&doc, Some("V2")).unwrap());
        let mut state = DefaultSelection::new(&doc, None, false, None).unwrap();
        assert!(!state.apply_personal(&doc, Some("V2"), false).unwrap());
        assert!(state.apply_resolved(&doc, Some("V1")).unwrap());
        assert!(!state.apply_personal(&doc, Some("V2"), false).unwrap());
        let mut state = DefaultSelection::new(&doc, None, false, None).unwrap();
        state.choose(&doc, None).unwrap();
        assert!(!state.apply_personal(&doc, Some("V1"), false).unwrap());
        assert_eq!(state.selected_id, None);
    }
    fn document() -> Document {
        Document::parse(json!({"id":"F_TEST","mode":"list","list_metadata":{"schema":[],"default_view":"V2","views":[
        {"id":"V0","name":"Record","type":"record","position":0},
        {"id":"V1","name":"All","type":"table","position":1,"default_view_key":"all_items"},
        {"id":"V2","name":"Other","type":"table","position":2}
    ]}})).unwrap()
    }
    #[test]
    fn resolved_defaults_only_replace_untouched_provisional_selection_once() {
        let doc = document();
        let mut state = DefaultSelection::new(&doc, None, false, None).unwrap();
        assert_eq!(state.selected_id.as_deref(), Some("V2"));
        assert!(state.apply_resolved(&doc, Some("V1")).unwrap());
        assert!(!state.apply_resolved(&doc, Some("V2")).unwrap());
        for selected in [Some("V2"), None] {
            let mut state = DefaultSelection::new(&doc, None, false, None).unwrap();
            state.choose(&doc, selected).unwrap();
            assert!(!state.apply_resolved(&doc, Some("V1")).unwrap());
            assert_eq!(state.selected_id.as_deref(), selected);
        }
        let mut explicit = DefaultSelection::new(&doc, Some("V2"), false, None).unwrap();
        assert!(!explicit.apply_resolved(&doc, Some("V1")).unwrap());
    }
    #[test]
    fn defaults_validate_ids_and_preserve_removed_selection_reason() {
        let mut doc = document();
        doc.views[1]["is_template_initial_view"] = json!(true);
        assert_eq!(
            DefaultSelection::new(&doc, None, false, None)
                .unwrap()
                .selected_id
                .as_deref(),
            Some("V1")
        );
        assert_eq!(
            DefaultSelection::new(&doc, Some("V2"), false, None)
                .unwrap()
                .selected_id
                .as_deref(),
            Some("V2")
        );
        let mut state = DefaultSelection::new(&doc, None, false, None).unwrap();
        assert!(state.apply_resolved(&doc, Some("MISSING")).is_err());
        assert!(state.apply_resolved(&doc, Some("V0")).is_err());
        doc.views.retain(|v| v["id"] != "V1");
        assert!(state.reconcile(&doc).is_err());
        assert_eq!(state.selected_id.as_deref(), Some("V1"));
    }
    #[test]
    fn completion_policy_distinguishes_unknown_off_and_personal_override() {
        assert!(
            CompletionPolicy::resolve(true, None, Some(false), None, Some(false), None).is_err()
        );
        assert!(
            !CompletionPolicy::resolve(true, Some(false), None, None, None, None)
                .unwrap()
                .apply_rules
        );
        assert!(
            !CompletionPolicy::resolve(false, None, None, None, None, None)
                .unwrap()
                .apply_rules
        );
        assert!(CompletionPolicy::resolve(true, Some(true), None, None, None, None).is_err());
        assert!(CompletionPolicy::resolve(true, Some(true), Some(true), None, None, None).is_err());
        assert!(
            CompletionPolicy::resolve(
                true,
                Some(true),
                Some(true),
                Some(false),
                Some(false),
                Some(false)
            )
            .unwrap()
            .effective_show_completed
        );
        assert!(
            !CompletionPolicy::resolve(true, Some(true), Some(true), Some(true), Some(false), None)
                .unwrap()
                .effective_show_completed
        );
        assert!(
            CompletionPolicy::resolve(true, Some(true), Some(false), None, Some(false), Some(true))
                .unwrap()
                .effective_show_completed
        );
    }
}
