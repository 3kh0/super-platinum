//! Experimental direct Person scan: no snapshot, cache, worker or mutation hooks.
use crate::{
    composer_completion::{Broadcast, Kind, LIMIT, Suggestion, Target, Trigger},
    state::Workspace,
};

#[derive(Clone, Copy)]
enum BorrowedTarget<'a> {
    User(&'a str),
    Group(&'a str),
    Broadcast(Broadcast),
}
impl BorrowedTarget<'_> {
    fn token(self, scratch: &mut String) -> bool {
        scratch.clear();
        let (prefix, id) = match self {
            Self::User(id) => ("<@", id),
            Self::Group(id) => ("<!subteam^", id),
            Self::Broadcast(b) => {
                scratch.push_str(match b {
                    Broadcast::Here => "<!here>",
                    Broadcast::Channel => "<!channel>",
                    Broadcast::Everyone => "<!everyone>",
                });
                return true;
            }
        };
        if id.is_empty() || !id.bytes().all(|c| c.is_ascii_alphanumeric()) {
            return false;
        }
        scratch.push_str(prefix);
        scratch.push_str(id);
        scratch.push('>');
        true
    }
    fn owned(self) -> Target {
        match self {
            Self::User(id) => Target::User(id.to_owned()),
            Self::Group(id) => Target::Group(id.to_owned()),
            Self::Broadcast(b) => Target::Broadcast(b),
        }
    }
}

#[derive(Clone, Copy)]
enum Detail<'a> {
    Handle(&'a str),
    Plain(&'a str),
}
impl Detail<'_> {
    fn owned(self) -> String {
        match self {
            Self::Handle(value) => format!("@{value}"),
            Self::Plain(value) => value.to_owned(),
        }
    }
}

struct Pick<'a> {
    exact_miss: bool,
    prefix_miss: bool,
    category: u8,
    folded_label: String,
    token: String,
    label: &'a str,
    detail: Detail<'a>,
    target: BorrowedTarget<'a>,
}
impl Pick<'_> {
    fn rank(&self) -> (bool, bool, u8, &str, &str) {
        (
            self.exact_miss,
            self.prefix_miss,
            self.category,
            &self.folded_label,
            &self.token,
        )
    }
    fn owned(self) -> Suggestion {
        Suggestion {
            label: format!("@{}", self.label),
            detail: self.detail.owned(),
            target: self.target.owned(),
        }
    }
}

/// Preserve str::to_lowercase's contextual Unicode behavior, including final
/// sigma. The ASCII branch reuses capacity; Unicode intentionally still allocates.
fn fold(value: &str, scratch: &mut String) {
    scratch.clear();
    if value.is_ascii() {
        scratch.push_str(value);
        scratch.make_ascii_lowercase();
    } else {
        scratch.push_str(&value.to_lowercase());
    }
}

/// Same lookup and fallback as Workspace::display_name, including malformed
/// map-key/user.id aliases. Only the final eight rows need an owned label.
fn display_name<'a>(workspace: &'a Workspace, id: &'a str) -> &'a str {
    let Some(user) = workspace.users.get(id) else {
        return id;
    };
    [
        user.profile
            .as_ref()
            .and_then(|p| p.display_name.as_deref()),
        user.profile.as_ref().and_then(|p| p.real_name.as_deref()),
        user.real_name.as_deref(),
        user.name.as_deref(),
    ]
    .into_iter()
    .flatten()
    .map(str::trim)
    .find(|name| !name.is_empty())
    .unwrap_or(id)
}

struct Scan<'a> {
    query: String,
    best: Vec<Pick<'a>>,
    names: [String; 3],
    label_scratch: String,
    token_scratch: String,
}
impl<'a> Scan<'a> {
    fn add(
        &mut self,
        label: &'a str,
        detail: Detail<'a>,
        target: BorrowedTarget<'a>,
        raw: [&str; 3],
        category: u8,
    ) {
        if !target.token(&mut self.token_scratch) {
            return;
        }
        for (value, scratch) in raw.into_iter().zip(&mut self.names) {
            fold(value, scratch);
        }
        let nonempty = || self.names.iter().filter(|name| !name.is_empty());
        if !self.query.is_empty() && !nonempty().any(|name| name.contains(&self.query)) {
            return;
        }
        let exact_miss = !nonempty().any(|name| name == &self.query);
        let prefix_miss = !nonempty().any(|name| name.starts_with(&self.query));
        // Lowercase the complete label, not individual Unicode scalars.
        self.label_scratch.clear();
        self.label_scratch.push('@');
        self.label_scratch.push_str(label);
        if self.label_scratch.is_ascii() {
            self.label_scratch.make_ascii_lowercase();
        } else {
            self.label_scratch = self.label_scratch.to_lowercase();
        }
        let rank = (
            exact_miss,
            prefix_miss,
            category,
            self.label_scratch.as_str(),
            self.token_scratch.as_str(),
        );
        // Strict comparison is essential: later equal ranks precede older ones.
        let position = self.best.partition_point(|old| old.rank() < rank);
        if position < LIMIT {
            self.best.insert(
                position,
                Pick {
                    exact_miss,
                    prefix_miss,
                    category,
                    folded_label: self.label_scratch.clone(),
                    token: self.token_scratch.clone(),
                    label,
                    detail,
                    target,
                },
            );
            self.best.truncate(LIMIT);
        }
    }
}

pub(super) fn suggestions(workspace: &Workspace, trigger: &Trigger) -> Vec<Suggestion> {
    debug_assert_eq!(trigger.kind, Kind::Person);
    let mut scan = Scan {
        query: trigger.query.to_lowercase(),
        best: Vec::with_capacity(LIMIT + 1),
        names: Default::default(),
        label_scratch: String::new(),
        token_scratch: String::new(),
    };
    for user in workspace.users.values().filter(|user| !user.deleted) {
        let name = display_name(workspace, &user.id);
        let handle = user.name.as_deref().unwrap_or_default();
        let real = user
            .real_name
            .as_deref()
            .or_else(|| user.profile.as_ref().and_then(|p| p.real_name.as_deref()))
            .unwrap_or_default();
        scan.add(
            name,
            if handle.is_empty() {
                Detail::Plain(real)
            } else {
                Detail::Handle(handle)
            },
            BorrowedTarget::User(&user.id),
            [name, handle, real],
            0,
        );
    }
    for group in workspace
        .usergroups
        .values()
        .filter(|group| group.date_delete == 0)
    {
        if !group.handle.is_empty() {
            scan.add(
                &group.handle,
                Detail::Plain(&group.name),
                BorrowedTarget::Group(&group.id),
                [&group.handle, &group.name, ""],
                1,
            );
        }
    }
    for (name, kind, detail) in [
        (
            "here",
            Broadcast::Here,
            "Notify people online in this conversation",
        ),
        (
            "channel",
            Broadcast::Channel,
            "Notify everyone in this conversation",
        ),
        (
            "everyone",
            Broadcast::Everyone,
            "Notify everyone in the workspace",
        ),
    ] {
        scan.add(
            name,
            Detail::Plain(detail),
            BorrowedTarget::Broadcast(kind),
            [name, "", ""],
            2,
        );
    }
    scan.best.into_iter().map(Pick::owned).collect()
}

#[cfg(test)]
mod tests;
