//! The distinct 8Bbq channel versus IM/MPIM picker filters. Posting facts apply
//! only to the missing-context normal-channel branch, not custom input or DMs.
use super::*;
use crate::block_actions::posting_policy::Flags;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ConversationKind {
    Channel,
    Direct,
    Group,
}
#[derive(Clone)]
pub struct DirectoryConversation {
    raw: Value,
    kind: ConversationKind,
}
impl DirectoryConversation {
    pub fn decode(raw: Value) -> Result<Self, Error> {
        if !raw.is_object()
            || !raw["id"].as_str().is_some_and(|v| {
                id(v) && matches!(v.as_bytes().first(), Some(b'C' | b'G' | b'D' | b'Z'))
            })
        {
            return Err(invalid());
        }
        for key in [
            "is_im",
            "is_mpim",
            "is_channel",
            "is_group",
            "is_private",
            "is_archived",
            "is_member",
            "is_frozen",
            "isUnknown",
            "isNonExistent",
        ] {
            if raw.get(key).is_some_and(|v| !v.is_boolean()) {
                return Err(invalid());
            }
        }
        let kind = if raw["is_im"] == true {
            ConversationKind::Direct
        } else if raw["is_mpim"] == true {
            ConversationKind::Group
        } else if raw["is_channel"] == true || raw["is_private"] == true || raw["is_group"] == true
        {
            ConversationKind::Channel
        } else {
            return Err(invalid());
        };
        Ok(Self { raw, kind })
    }
    pub fn id(&self) -> &str {
        self.raw["id"].as_str().unwrap()
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
    pub fn kind(&self) -> ConversationKind {
        self.kind
    }
    /// Owning-team filtering is a separate, acquired external-workspace fact.
    /// This predicate does not claim its enforcement or freshness.
    pub fn eligible(&self, custom: bool, flags: Option<Flags>) -> bool {
        if self.raw["isUnknown"] == true || self.raw["isNonExistent"] == true {
            return false;
        }
        if self.kind != ConversationKind::Channel {
            return true;
        }
        if self.raw["is_archived"] == true || self.raw["is_member"] != true {
            return false;
        }
        if custom {
            return true;
        }
        if self.raw["is_frozen"] == true {
            return false;
        }
        match self.raw.pointer("/properties/huddles/is_huddle_channel") {
            None | Some(Value::Bool(false)) => {}
            _ => return false,
        }
        flags.is_some_and(Flags::permits_root_post)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn custom_channel_and_direct_group_paths_do_not_import_context_posting_policy() {
        let c = DirectoryConversation::decode(
            json!({"id":"C1","is_channel":true,"is_member":true,"is_frozen":true}),
        )
        .unwrap();
        assert!(c.eligible(true, None));
        assert!(!c.eligible(
            false,
            Some(Flags {
                read_only: false,
                thread_only: false
            })
        ));
        for raw in [
            json!({"id":"D1","is_im":true}),
            json!({"id":"G1","is_mpim":true}),
        ] {
            let c = DirectoryConversation::decode(raw).unwrap();
            assert!(c.eligible(false, None));
        }
    }
    #[test]
    fn normal_channel_requires_membership_and_observed_effective_posting_facts() {
        let raw = json!({"id":"C1","is_channel":true,"is_member":true});
        let c = DirectoryConversation::decode(raw.clone()).unwrap();
        assert!(!c.eligible(false, None));
        assert!(c.eligible(
            false,
            Some(Flags {
                read_only: false,
                thread_only: false
            })
        ));
        for flags in [
            Flags {
                read_only: true,
                thread_only: false,
            },
            Flags {
                read_only: false,
                thread_only: true,
            },
        ] {
            assert!(!c.eligible(false, Some(flags)));
        }
        for key in ["is_archived", "isUnknown", "isNonExistent"] {
            let mut changed = raw.clone();
            changed[key] = json!(true);
            assert!(
                !DirectoryConversation::decode(changed)
                    .unwrap()
                    .eligible(true, None)
            );
        }
        let mut changed = raw;
        changed["is_member"] = json!(false);
        assert!(
            !DirectoryConversation::decode(changed)
                .unwrap()
                .eligible(true, None)
        );
    }
}
