//! Inert rich discussion presentation. No URL/file/control/navigation capability.
use super::*;
use crate::state::AttachmentVm;
fn inert(node: &RichNode, depth: usize) -> RichNode {
    if depth > 64 {
        return RichNode::Text("Content is available in the thread.".into());
    }
    let list = |nodes: &[RichNode]| nodes.iter().map(|n| inert(n, depth + 1)).collect();
    match node {
        RichNode::Table(table) => RichNode::Text(table.plain_text()),
        RichNode::Canvas(c) => RichNode::Text(c.title.clone()),
        RichNode::FileMention { fallback, .. } => RichNode::Text(if fallback.is_empty() {
            "File".into()
        } else {
            fallback.clone()
        }),
        RichNode::Link { label, url } => RichNode::Text(if label.is_empty() {
            url.clone()
        } else {
            label.clone()
        }),
        RichNode::UserMention { label, .. }
        | RichNode::GroupMention { label, .. }
        | RichNode::ChannelMention { label, .. } => RichNode::Text(label.clone()),
        RichNode::Button { label, .. } => RichNode::Text(label.clone()),
        RichNode::Media { name, .. } => RichNode::Text(name.clone()),
        RichNode::ImageBlock { id, alt, title, .. } => RichNode::Paragraph(vec![
            RichNode::Text(title.clone().unwrap_or_default()),
            RichNode::InlineImage {
                id: id.clone(),
                alt: alt.clone(),
            },
        ]),
        RichNode::Underlined(c) => RichNode::Underlined(Box::new(inert(c, depth + 1))),
        RichNode::Paragraph(c) => RichNode::Paragraph(list(c)),
        RichNode::Quote(c) => RichNode::Quote(list(c)),
        RichNode::Header(c) => RichNode::Header(list(c)),
        RichNode::Context(c) => RichNode::Context(list(c)),
        RichNode::Actions(c) => RichNode::Actions(list(c)),
        RichNode::Section {
            text,
            fields,
            accessory,
        } => RichNode::Section {
            text: list(text),
            fields: fields.iter().map(|f| list(f)).collect(),
            accessory: accessory.as_ref().map(|a| Box::new(inert(a, depth + 1))),
        },
        RichNode::List {
            ordered,
            indent,
            offset,
            items,
        } => RichNode::List {
            ordered: *ordered,
            indent: *indent,
            offset: *offset,
            items: items.iter().map(|i| list(i)).collect(),
        },
        RichNode::Text(_)
        | RichNode::StyledText { .. }
        | RichNode::SearchText { .. }
        | RichNode::Code(_)
        | RichNode::Emoji { .. }
        | RichNode::EmojiImage { .. }
        | RichNode::Divider
        | RichNode::InlineImage { .. } => node.clone(),
    }
}
pub(super) fn body(state: Signal<ShellState>, epoch: u64, node: &RichNode) -> Element {
    rich_node(&inert(node, 0), epoch, state)
}
pub(super) fn avatar(
    state: Signal<ShellState>,
    snapshot: &ShellState,
    message: &MessageVm,
) -> Element {
    rsx! {div{class:"avatar", "aria-hidden":"true",
        if let Some(id)=message.avatar.as_ref().filter(|id|snapshot.media.is_ready(id)){
            crate::view::media_image::MediaImage{state,id:id.clone(),media_epoch:snapshot.media_epoch,compact:true,alt:message.author.clone(),fallback:rsx!{"{message.avatar_initials}"}}
        }else{"{message.avatar_initials}"}
    }}
}
pub(super) fn meta(message: &MessageVm) -> Element {
    rsx! {div{class:"message-meta",strong{"{message.author}"}if message.is_app{span{class:"app-badge","APP"}}time{"{message.timestamp}"}if message.edited{span{"(edited)"}}}}
}
pub(super) fn attachments(
    state: Signal<ShellState>,
    epoch: u64,
    values: &[AttachmentVm],
) -> Element {
    rsx! {div{class:"message-attachments",
        for value in values{section{class:"attachment",key:"{value.key}",
            for n in &value.pretext{{body(state,epoch,n)}}
            if let Some(author)=&value.author{div{class:"attachment-author","{author.name}"}}
            if let Some(title)=&value.title{strong{class:"attachment-title","{title}"}}
            for n in &value.body{{body(state,epoch,n)}}
            for (label,nodes,_) in &value.fields{div{class:"attachment-field",strong{"{label}"}for n in nodes{{body(state,epoch,n)}}}}
            for n in &value.files{{body(state,epoch,n)}}
            if let Some(footer)=&value.footer{div{class:"attachment-footer","{footer.lead} {footer.channel_label.clone().unwrap_or_default()} {footer.stamp.clone().unwrap_or_default()}"}}
        }}
    }}
}
#[cfg(test)]
mod tests;
