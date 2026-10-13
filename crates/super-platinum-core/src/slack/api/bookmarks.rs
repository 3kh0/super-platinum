//! Desktop bookmark parameters verified against Slack's shipped client.
use super::*;
use crate::bookmarks::{Bookmark, valid_link};
pub fn bookmarks_list(
    client: &SlackClient,
    workspace: &WorkspaceSession,
    channel: &str,
) -> PreparedRequest {
    client.rest_form(
        workspace,
        "bookmarks.list",
        vec![
            ("channel", channel.into()),
            ("include_folders", "true".into()),
            ("include_legacy_workflows", "true".into()),
        ],
    )
}
pub async fn fetch_bookmarks(
    t: &Transport,
    c: &SlackClient,
    w: &WorkspaceSession,
    channel: &str,
) -> Result<Vec<Bookmark>, Error> {
    let response = t.execute(bookmarks_list(c, w, channel)).await?;
    decode(
        response
            .get("bookmarks")
            .cloned()
            .ok_or_else(|| Error::Api("bookmarks_missing".into()))?,
        "bookmarks",
    )
}
pub fn bookmark_save(
    c: &SlackClient,
    w: &WorkspaceSession,
    channel: &str,
    id: Option<&str>,
    title: &str,
    link: &str,
) -> Result<PreparedRequest, Error> {
    if title.trim().is_empty() || !valid_link(link) {
        return Err(Error::Api("invalid_bookmark".into()));
    }
    let mut fields = vec![
        ("channel", channel.into()),
        ("type", "link".into()),
        ("title", title.trim().into()),
        ("link", link.trim().into()),
    ];
    if let Some(id) = id {
        fields.push(("bookmark", id.into()));
    }
    // Omitting emoji/icon/parent keeps the existing bookmark metadata intact.
    Ok(c.rest_form(
        w,
        if id.is_some() {
            "bookmarks.edit"
        } else {
            "bookmarks.add"
        },
        fields,
    ))
}
pub fn bookmark_remove(
    c: &SlackClient,
    w: &WorkspaceSession,
    channel: &str,
    id: &str,
) -> PreparedRequest {
    c.rest_form(
        w,
        "bookmarks.remove",
        vec![
            ("channel", channel.into()),
            ("bookmark", id.into()),
            ("type", "link".into()),
        ],
    )
}

pub fn bookmark_folder_save(
    c: &SlackClient,
    w: &WorkspaceSession,
    channel: &str,
    id: Option<&str>,
    title: &str,
) -> Result<PreparedRequest, Error> {
    if title.trim().is_empty() {
        return Err(Error::Api("invalid_bookmark".into()));
    }
    let mut fields = vec![
        ("channel", channel.into()),
        ("title", title.trim().into()),
        ("type", "local_folder".into()),
    ];
    if let Some(id) = id {
        fields.push(("bookmark", id.into()));
    }
    Ok(c.rest_form(
        w,
        if id.is_some() {
            "bookmarks.edit"
        } else {
            "bookmarks.add"
        },
        fields,
    ))
}
pub fn bookmark_folder_remove(
    c: &SlackClient,
    w: &WorkspaceSession,
    channel: &str,
    id: &str,
) -> PreparedRequest {
    c.rest_form(
        w,
        "bookmarks.remove",
        vec![
            ("channel", channel.into()),
            ("bookmark", id.into()),
            ("type", "local_folder".into()),
        ],
    )
}
pub fn bookmark_reorder(
    c: &SlackClient,
    w: &WorkspaceSession,
    channel: &str,
    bookmark: &Bookmark,
    placement: &crate::bookmarks::Placement,
) -> Result<PreparedRequest, Error> {
    if !bookmark.editable() {
        return Err(Error::Api("invalid_bookmark".into()));
    }
    let mut fields = vec![
        ("channel", channel.into()),
        ("center", bookmark.id.clone()),
        (
            "bookmark_type",
            if bookmark.folder() {
                "local_folder"
            } else {
                "link"
            }
            .into(),
        ),
    ];
    // Top-level placement omits parent_id. Sending an empty string is rejected
    // with invalid_parent_type by the desktop API (private round-trip verified).
    if let Some(parent) = &placement.parent {
        fields.push(("parent_id", parent.clone()));
    }
    if let Some(left) = &placement.left {
        fields.push(("left", left.clone()));
    }
    if let Some(right) = &placement.right {
        fields.push(("right", right.clone()));
    }
    Ok(c.rest_form(w, "bookmarks.reorder", fields))
}
