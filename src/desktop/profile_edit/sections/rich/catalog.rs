use super::*;
#[derive(Clone, Default)]
pub(super) struct Catalog {
    entries: Vec<(Value, String)>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Witness {
    pub value: Value,
    pub label: String,
}
impl Catalog {
    pub(super) fn contains_any(&self, ids: &[String]) -> bool {
        self.entries.iter().any(|(token, _)| {
            token["type"] == "channel"
                && token["channel_id"]
                    .as_str()
                    .is_some_and(|id| ids.iter().any(|failed| failed == id))
        })
    }
    pub(super) fn insert(&mut self, raw: &Value, label: String) -> Result<(), Error> {
        self.entries.push((identity(raw)?, label));
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
                Some("channel" | "user" | "usergroup")
            )
        }) {
            let identity = identity(&token.value)?;
            if !self
                .entries
                .iter()
                .any(|(raw, label)| raw == &identity && label == &token.label)
            {
                return Err(unavailable());
            }
            context.mention(&token.value, token.label.clone())?;
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
                value: user.clone(),
                label: "@".into(),
            },
            Witness {
                value: channel.clone(),
                label: "#channel".into(),
            },
            Witness {
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
                value: channel.clone(),
                label: "#channel".into(),
            },
            Witness {
                value: user.clone(),
                label: "@".into(),
            },
            Witness {
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
}
