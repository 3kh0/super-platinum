//! Freshness witnesses for accepted Init/Extras reducers. These containers are
//! NOT complete name/assignment observations and can never populate Complete.
use serde_json::Value;
use std::collections::BTreeMap;
use super_platinum_core::state::cfm_payload::Stores;

const USER: [&str; 3] = ["display_real_names_override", "hidden_users", "frecency"];
pub(super) struct Inputs {
    user: [Option<Value>; 3],
    team: BTreeMap<String, Option<Value>>,
    hidden: BTreeMap<String, Value>,
    internal: Option<Value>,
    default: Option<Value>,
}
impl Inputs {
    pub(super) fn capture(s: &Stores) -> Self {
        Self {
            user: USER.map(|k| s.user_prefs.get(k).cloned()),
            team: s
                .team_prefs
                .iter()
                .map(|(k, v)| (k.clone(), v.get("display_real_names").cloned()))
                .collect(),
            hidden: s.hidden_users.clone(),
            internal: s.internal_workspace_ids.clone(),
            default: s.default_workspace.clone(),
        }
    }
    pub(super) fn current(&self, s: &Stores) -> bool {
        USER.iter()
            .zip(&self.user)
            .all(|(k, v)| s.user_prefs.get(*k) == v.as_ref())
            && self.team.len() == s.team_prefs.len()
            && self.team.iter().all(|(k, v)| {
                s.team_prefs
                    .get(k)
                    .is_some_and(|actual| actual.get("display_real_names") == v.as_ref())
            })
            && self.hidden == s.hidden_users
            && self.internal == s.internal_workspace_ids
            && self.default == s.default_workspace
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn only_relevant_accepted_containers_retire_the_witness() {
        let mut s = Stores::default();
        s.team_prefs.insert("T1".into(), BTreeMap::new());
        let before = Inputs::capture(&s);
        s.user_prefs.insert("unrelated".into(), json!(true));
        s.team_prefs
            .get_mut("T1")
            .unwrap()
            .insert("unrelated".into(), json!(true));
        s.muted_channels.insert("C1".into(), json!(true));
        assert!(before.current(&s));
        for index in 0..7 {
            let mut changed = s.clone();
            match index {
                0..=2 => {
                    changed.user_prefs.insert(USER[index].into(), json!(true));
                }
                3 => {
                    changed
                        .team_prefs
                        .get_mut("T1")
                        .unwrap()
                        .insert("display_real_names".into(), json!(true));
                }
                4 => {
                    changed.hidden_users.insert("U1".into(), json!(true));
                }
                5 => changed.internal_workspace_ids = Some(json!(["T1"])),
                _ => changed.default_workspace = Some(json!("T1")),
            }
            assert!(!before.current(&changed), "field {index}");
        }
    }
}
