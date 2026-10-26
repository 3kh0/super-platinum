use super::*;
#[derive(Clone, Default)]
pub(super) struct Catalog {
    entries: Vec<(Value, String, Option<Presentation>)>,
    presentations: Vec<(Value, Presentation)>,
    section_units: Vec<(Value, Vec<u16>)>,
    channel_dependencies: std::collections::BTreeSet<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Witness {
    pub value: Value,
    #[serde(default)]
    pub label: String,
    #[serde(default, rename = "labelUnits")]
    pub label_units: Option<Vec<u16>>,
    #[serde(default)]
    pub presentation: Option<Presentation>,
}
/// Presentation is independently admitted from a current Canvas status receipt.
/// These bits never enter the persisted raw token.
#[derive(Clone, Debug, serde::Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Presentation {
    kind: String,
    icon: String,
    disabled: bool,
}
impl Presentation {
    pub(super) fn canvas_section() -> Self {
        Self {
            kind: "canvas-section".into(),
            icon: "description".into(),
            disabled: false,
        }
    }
    pub(super) fn deleted_canvas() -> Self {
        Self {
            kind: "deleted-canvas".into(),
            icon: "delete".into(),
            disabled: true,
        }
    }
    pub(super) fn private_canvas() -> Self {
        Self {
            kind: "private-canvas".into(),
            icon: "lock".into(),
            disabled: false,
        }
    }
}
impl Catalog {
    pub(super) fn contains_any(&self, ids: &[String]) -> bool {
        ids.iter().any(|id| self.channel_dependencies.contains(id))
            || self.entries.iter().any(|(token, _, _)| {
                token["type"] == "channel"
                    && token["channel_id"]
                        .as_str()
                        .is_some_and(|id| ids.iter().any(|failed| failed == id))
            })
    }
    pub(super) fn channel_dependencies(&mut self, ids: impl IntoIterator<Item = String>) {
        self.channel_dependencies.extend(ids);
    }
    pub(super) fn private_canvas(&mut self, raw: &Value) -> Result<(), Error> {
        self.reference_presentation(raw, Presentation::private_canvas())
    }
    pub(super) fn deleted_canvas(&mut self, raw: &Value) -> Result<(), Error> {
        self.reference_presentation(raw, Presentation::deleted_canvas())
    }
    fn reference_presentation(
        &mut self,
        raw: &Value,
        presentation: Presentation,
    ) -> Result<(), Error> {
        if raw["type"] != "canvas" {
            return Err(unavailable());
        }
        let identity = identity(raw)?;
        let entry = self
            .entries
            .iter_mut()
            .rev()
            .find(|(value, _, _)| value == &identity)
            .ok_or_else(unavailable)?;
        entry.2 = Some(presentation.clone());
        self.presentations.push((raw.clone(), presentation));
        Ok(())
    }
    pub(super) fn presentations(&self) -> Value {
        Value::Object(
            self.presentations
                .iter()
                .map(|(raw, presentation)| {
                    (raw.to_string(), serde_json::to_value(presentation).unwrap())
                })
                .collect(),
        )
    }
    pub(super) fn insert_section(&mut self, raw: &Value, units: &[u16]) -> Result<(), Error> {
        self.insert(raw, String::from_utf16_lossy(units))?;
        self.section_units.push((identity(raw)?, units.to_vec()));
        self.reference_presentation(raw, Presentation::canvas_section())
    }
    pub(super) fn label_units(&self) -> Value {
        Value::Object(
            self.presentations
                .iter()
                .filter_map(|(raw, _)| {
                    let key = identity(raw).ok()?;
                    let (_, units) = self.section_units.iter().find(|(id, _)| id == &key)?;
                    Some((raw.to_string(), serde_json::json!(units)))
                })
                .collect(),
        )
    }
    pub(super) fn insert(&mut self, raw: &Value, label: String) -> Result<(), Error> {
        self.entries.push((identity(raw)?, label, None));
        Ok(())
    }
    pub(super) fn context(
        &self,
        tokens: &[Witness],
        owner: &Owner,
    ) -> Result<protocol::RichTextContext, Error> {
        let mut context = protocol::RichTextContext::new_ordered(&owner.team, &owner.user)?;
        for token in tokens.iter().filter(|t| {
            matches!(
                t.value["type"].as_str(),
                Some("channel" | "user" | "usergroup" | "file" | "canvas")
            )
        }) {
            let identity = identity(&token.value)?;
            let section = self.section_units.iter().find(|(raw, _)| raw == &identity);
            let label_matches = |label: &String| match section {
                Some((_, units)) => {
                    token.label.is_empty() && token.label_units.as_ref() == Some(units)
                }
                None => token.label_units.is_none() && label == &token.label,
            };
            if !self.entries.iter().any(|(raw, label, presentation)| {
                raw == &identity && label_matches(label) && presentation == &token.presentation
            }) {
                return Err(unavailable());
            }
            if let Some((_, units)) = section {
                context.canvas_section(&token.value, units)?;
            } else if matches!(token.value["type"].as_str(), Some("file" | "canvas")) {
                context.file(&token.value, token.label.clone())?;
            } else {
                context.mention(&token.value, token.label.clone())?;
            }
        }
        Ok(context)
    }
}
fn identity(value: &Value) -> Result<Value, Error> {
    if !match value["type"].as_str() {
        Some("channel") => canonical::ordinary_tab(value),
        Some("user") => value["user_id"].as_str().is_some_and(|id| !id.is_empty()),
        Some("usergroup") => value["usergroup_id"]
            .as_str()
            .is_some_and(|id| !id.is_empty()),
        Some("file" | "canvas") => value["file_id"].as_str().is_some_and(|id| !id.is_empty()),
        _ => false,
    } {
        return Err(unavailable());
    }
    let mut value = value.clone();
    value
        .as_object_mut()
        .ok_or_else(unavailable)?
        .remove("style");
    Ok(value)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn section_witness_requires_exact_units_even_for_equal_lossy_display() {
        let raw = serde_json::json!({"type":"canvas","file_id":"FFILE0001","section_id":"temp:C:0123456789abcdef01234567","url":"https://fixture.invalid/canvas","opaque":{"keep":7},"style":{"bold":true}});
        let owner = Owner {
            account: "account".into(),
            team: "T12345678".into(),
            user: "U12345678".into(),
        };
        let first = vec![b'A' as u16, 0xd83d, 0x2026];
        let other = vec![b'A' as u16, 0xd83e, 0x2026];
        assert_eq!(
            String::from_utf16_lossy(&first),
            String::from_utf16_lossy(&other)
        );
        let mut catalog = Catalog::default();
        catalog.insert_section(&raw, &first).unwrap();
        let mut witness = Witness {
            value: raw.clone(),
            label: String::new(),
            label_units: Some(first.clone()),
            presentation: Some(Presentation::canvas_section()),
        };
        assert!(
            catalog
                .context(std::slice::from_ref(&witness), &owner)
                .is_ok()
        );
        witness.label_units = Some(other);
        assert!(
            catalog
                .context(std::slice::from_ref(&witness), &owner)
                .is_err()
        );
        witness.label_units = None;
        witness.label = String::from_utf16_lossy(&first);
        assert!(
            catalog
                .context(std::slice::from_ref(&witness), &owner)
                .is_err()
        );
        witness.label_units = Some(first);
        witness.label.clear();
        witness.presentation = None;
        assert!(
            catalog
                .context(std::slice::from_ref(&witness), &owner)
                .is_err()
        );
        witness.presentation = Some(Presentation::canvas_section());
        witness.value["section_id"] = Value::from("other");
        assert!(catalog.context(&[witness], &owner).is_err());
        assert_eq!(
            catalog.label_units()[raw.to_string()],
            serde_json::json!([65, 55357, 8230])
        );
    }
    #[test]
    fn only_style_is_outside_catalog_identity() {
        let a = serde_json::json!({"type":"channel","channel_id":"C12345678","future":{"x":1}});
        let mut b = a.clone();
        b["style"] = serde_json::json!({"bold":true});
        assert_eq!(identity(&a).unwrap(), identity(&b).unwrap());
        b["future"]["x"] = Value::from(2);
        assert_ne!(identity(&a).unwrap(), identity(&b).unwrap());
    }
    #[test]
    fn admitted_labels_survive_source_changes_but_unknown_data_cannot_change() {
        let raw = serde_json::json!({"type":"channel","channel_id":"C12345678","future":1});
        let mut catalog = Catalog::default();
        catalog.insert(&raw, "#at-opening".into()).unwrap();
        let owner = Owner {
            account: "account".into(),
            team: "T12345678".into(),
            user: "U12345678".into(),
        };
        let mut token = Witness {
            label_units: None,
            presentation: None,
            value: raw,
            label: "#at-opening".into(),
        };
        assert!(
            catalog
                .context(std::slice::from_ref(&token), &owner)
                .is_ok()
        );
        token.label = "#later-name".into();
        assert!(
            catalog
                .context(std::slice::from_ref(&token), &owner)
                .is_err()
        );
        token.label = "#at-opening".into();
        token.value["future"] = Value::from(2);
        assert!(catalog.context(&[token], &owner).is_err());
    }
    #[test]
    fn mixed_mentions_keep_capture_order_and_exact_user_metadata() {
        let channel = serde_json::json!({"type":"channel","channel_id":"C12345678","future":1});
        let user = serde_json::json!({"type":"user","user_id":"UMENTION1","future":2,"channel_id":"opaque-user-data"});
        let mut catalog = Catalog::default();
        catalog.insert(&channel, "#channel".into()).unwrap();
        catalog.insert(&user, "@".into()).unwrap();
        let mut styled = user.clone();
        styled["style"] = serde_json::json!({"bold":true});
        let witnesses = vec![
            Witness {
                label_units: None,
                presentation: None,
                value: user.clone(),
                label: "@".into(),
            },
            Witness {
                label_units: None,
                presentation: None,
                value: channel.clone(),
                label: "#channel".into(),
            },
            Witness {
                label_units: None,
                presentation: None,
                value: styled.clone(),
                label: "@".into(),
            },
        ];
        let blocks = vec![
            serde_json::json!({"type":"rich_text","elements":[{"type":"rich_text_section","elements":[user,channel,styled]}]}),
        ];
        let mut shell = ShellState::fixture(crate::media::MediaRegistry::default());
        fixture::install_interaction(&mut shell);
        fixture::mentions::set_blocks(&mut shell, blocks.clone());
        let mut edited = blocks.clone();
        edited[0]["elements"][0]["elements"]
            .as_array_mut()
            .unwrap()
            .insert(0, serde_json::json!({"type":"text","text":"Changed "}));
        // Match the actual synthetic section owner while preserving the catalog.
        let owner = Owner::capture(&shell).unwrap();
        let context = catalog.context(&witnesses, &owner).unwrap();
        let form = shell
            .profile_edit
            .sections
            .draft
            .as_mut()
            .unwrap()
            .form
            .as_mut()
            .unwrap();
        assert!(form.change_rich("long", edited.clone(), context).is_ok());
        assert!(form.validation("long").is_ok());
        assert!(form.capture().is_ok());
        let reordered = vec![
            Witness {
                label_units: None,
                presentation: None,
                value: channel.clone(),
                label: "#channel".into(),
            },
            Witness {
                label_units: None,
                presentation: None,
                value: user.clone(),
                label: "@".into(),
            },
            Witness {
                label_units: None,
                presentation: None,
                value: styled.clone(),
                label: "@".into(),
            },
        ];
        let wrong = catalog.context(&reordered, &owner).unwrap();
        // The form keeps invalid edits visible; validation/save reject the
        // reordered custody, without throwing away the user's current text.
        assert!(form.change_rich("long", edited.clone(), wrong).is_ok());
        assert!(form.validation("long").is_err());
        assert!(form.capture().is_err());
        assert!(form.value("long") == Some(&SectionValue::LongText(edited)));
        assert!(!catalog.contains_any(&["opaque-user-data".into()]));
        let mut changed = witnesses;
        changed[0].value["future"] = Value::from(3);
        assert!(catalog.context(&changed, &owner).is_err());
    }

    #[test]
    fn file_labels_require_catalog_admission_and_preserve_wire_identity() {
        let owner = Owner {
            account: "account".into(),
            team: "T12345678".into(),
            user: "U12345678".into(),
        };
        for kind in ["file", "canvas"] {
            let raw = serde_json::json!({
                "type":kind,"file_id":"FFILE0001",
                "text":"https://fixture.invalid/document",
                "url":"https://fixture.invalid/document", "opaque":{"keep":1}
            });
            let mut catalog = Catalog::default();
            let mut token = Witness {
                label_units: None,
                presentation: None,
                value: raw.clone(),
                label: "Canonical title".into(),
            };
            assert!(
                catalog
                    .context(std::slice::from_ref(&token), &owner)
                    .is_err()
            );
            catalog.insert(&raw, token.label.clone()).unwrap();
            token.value["style"] = serde_json::json!({"bold":true});
            assert!(
                catalog
                    .context(std::slice::from_ref(&token), &owner)
                    .is_ok()
            );
            token.label = "Wire text cannot replace the title".into();
            assert!(
                catalog
                    .context(std::slice::from_ref(&token), &owner)
                    .is_err()
            );
            token.label = "Canonical title".into();
            token.value["opaque"]["keep"] = Value::from(2);
            assert!(
                catalog
                    .context(std::slice::from_ref(&token), &owner)
                    .is_err()
            );
            token.value = raw;
            token.value["url"] = Value::from("https://fixture.invalid/changed");
            assert!(catalog.context(&[token], &owner).is_err());
        }
    }
}
