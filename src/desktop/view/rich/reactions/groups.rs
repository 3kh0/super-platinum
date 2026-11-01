//! Ordinary passive grouping: hTYg/RiSH name prefix, then cV5F stable groups.
//! This presentation consumes the caller's existing guarded native projection.
//! It supplies no keyed Source-store, freshness, resource or action authority.
use super::*;
use crate::model::ReactorVm;
use std::collections::HashMap;

pub(crate) struct Group<'a> {
    pub(crate) name: &'a str,
    pub(crate) count: u64,
    pub(crate) own: bool,
    pub(crate) variants: Vec<&'a ReactionVm>,
    people: Vec<&'a ReactorVm>,
}
pub(crate) fn groups(reactions: &[ReactionVm]) -> Vec<Group<'_>> {
    let mut positions = HashMap::new();
    let mut groups: Vec<Group<'_>> = Vec::new();
    for reaction in reactions {
        // Ordinary Source does NOT use the card parser, lowercase, resolve
        // aliases or validate skin-tone suffixes here.
        let name = reaction.name.split("::").next().unwrap_or("");
        let index = *positions.entry(name).or_insert_with(|| {
            let index = groups.len();
            groups.push(Group {
                name,
                count: 0,
                own: false,
                variants: Vec::new(),
                people: Vec::new(),
            });
            index
        });
        let group = &mut groups[index];
        group.count = group.count.saturating_add(u64::from(reaction.count));
        group.own |= reaction.own;
        group.variants.push(reaction);
        for person in &reaction.reactors {
            if group.people.len() < crate::model::REACTOR_NAMES_SHOWN
                && !group.people.iter().any(|old| old.user_id == person.user_id)
            {
                group.people.push(person);
            }
        }
    }
    groups
}
impl Group<'_> {
    pub(crate) fn label(&self) -> String {
        let names = self
            .people
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>();
        let variants = if self.variants.len() > 1 {
            format!(" ({} emoji variants)", self.variants.len())
        } else {
            String::new()
        };
        // A person can use several variants. The summed reaction counter is
        // not a unique-person total; never infer phantom "other" people from it.
        // This native label is separate from Source's generated-tooltip service.
        let people = if names.is_empty() {
            String::new()
        } else {
            format!(" \u{b7} {}", names.join(", "))
        };
        format!(
            "{} {} with :{}:{variants}{people}",
            self.count,
            if self.count == 1 {
                "reaction"
            } else {
                "reactions"
            },
            self.name
        )
    }
}
pub(crate) fn reaction_groups_read_only(
    state: Signal<ShellState>,
    media: &crate::media::MediaRegistry,
    media_epoch: u64,
    reactions: &[ReactionVm],
) -> Element {
    rsx! {for (index,group) in groups(reactions).iter().enumerate() {
        span {
            key:"reaction-group-{index}-{group.name}",
            "data-reaction-name":"{group.name}",
            class:if group.own{"reaction reaction-read-only own"}else{"reaction reaction-read-only"},
            title:group.label(),"aria-label":group.label(),
            span {class:"reaction-variants",
                for variant in group.variants.iter().take(10) {
                    {super::reaction_emoji(state,media,media_epoch,variant,"reaction-emoji","reaction-glyph")}
                }
                if group.variants.len()>10 {span {
                    class:"reaction-variants-overflow",title:format!("{} more emoji variants",group.variants.len()-10),
                    {crate::icons::icon(crate::icons::Icon::More,"icon sm")}
                }}
            }
            span {class:"reaction-count","{group.count}"}
        }
    }}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn search_reaction_groups_keep_source_prefix_order_counts_people_and_variants_without_alias_merging()
     {
        let reaction = |name: &str, count, own, users: &[&str]| ReactionVm {
            name: name.into(),
            count,
            own,
            reactors: users
                .iter()
                .map(|id| ReactorVm {
                    user_id: (*id).into(),
                    name: (*id).into(),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        let rows = vec![
            reaction("thumbsup::skin-tone-2", u32::MAX, false, &["U1", "U2"]),
            reaction("custom", 0, false, &[]),
            reaction("thumbsup::not-a-tone", u32::MAX, true, &["U2", "U3"]),
            reaction("+1", 1, false, &["U1"]),
            reaction("Thumbsup", 1, false, &["U4"]),
            reaction("thumbsup", 0, false, &["U1"]),
        ];
        let got = groups(&rows);
        assert_eq!(
            got.iter().map(|g| (g.name, g.count)).collect::<Vec<_>>(),
            [
                ("thumbsup", 2 * u64::from(u32::MAX)),
                ("custom", 0),
                ("+1", 1),
                ("Thumbsup", 1)
            ]
        );
        assert!(got[0].own);
        assert_eq!(
            got[0]
                .variants
                .iter()
                .map(|r| r.name.as_str())
                .collect::<Vec<_>>(),
            ["thumbsup::skin-tone-2", "thumbsup::not-a-tone", "thumbsup"]
        );
        assert_eq!(
            got[0]
                .people
                .iter()
                .map(|p| p.user_id.as_str())
                .collect::<Vec<_>>(),
            ["U1", "U2", "U3"]
        );
        assert!(got[1].label().starts_with("0 reactions"));
        assert!(!got[0].label().contains("other"));
        assert!(groups(&[]).is_empty());
    }
}
