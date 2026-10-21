//! Painted title fragments are distinct from the canonical semantic label.
//! This reuses the StepText emoji lookup; it never decodes entities again.
use super::*;
use crate::media::MediaUri;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub(super) enum Fragment {
    Text {
        text: String,
    },
    Emoji {
        text: String,
        title: String,
    },
    Image {
        src: String,
        alt: String,
        title: String,
        #[serde(skip)]
        id: MediaAssetId,
        #[serde(skip)]
        epoch: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Painted {
    pub(super) label: String,
    pub(super) fragments: Vec<Fragment>,
}

impl Painted {
    pub(super) fn project(s: &ShellState, team: &str, label: &str, epoch: u64) -> Self {
        let fragments = tokens::split(label)
            .into_iter()
            .map(|(emoji, text)| {
                let inline = if emoji {
                    single(s, team, text, true)
                } else {
                    Inline::Text(text.into())
                };
                match inline {
                    Inline::Emoji { text, title, image } => {
                        let title = title.unwrap_or_default();
                        match image.filter(|id| s.media.is_ready(id) && !s.media.image_state(id).1)
                        {
                            Some(id) => Fragment::Image {
                                src: id.webview_uri_at(epoch),
                                alt: text,
                                title,
                                id,
                                epoch,
                            },
                            None => Fragment::Emoji { text, title },
                        }
                    }
                    Inline::Text(text) => Fragment::Text { text },
                    _ => unreachable!("single emoji lookup cannot return an entity"),
                }
            })
            .collect();
        Self {
            label: label.into(),
            fragments,
        }
    }

    pub(super) fn current(&self, s: &ShellState, team: &str) -> bool {
        *self == Self::project(s, team, &self.label, s.media_epoch)
    }

    pub(super) fn render(&self, state: Signal<ShellState>) -> Element {
        rsx! {
            span {class:"workflow-activity-file-name", "data-file-label":self.label.clone(),
                for (index, fragment) in self.fragments.iter().enumerate() {
                    {match fragment {
                        Fragment::Text{text} => rsx! {
                            span {"data-file-title-kind":"text","data-file-title-index":index,"{text}"}
                        },
                        Fragment::Emoji{text,title} => rsx! {
                            span {class:"custom-emoji","data-file-title-kind":"emoji","data-file-title-index":index,title:title.clone(),"{text}"}
                        },
                        Fragment::Image{id,epoch,alt,title,..} => {
                            let fallback = rsx! {span {class:"custom-emoji",title:title.clone(),"{alt}"}};
                            rsx! {
                                span {"data-file-title-kind":"image","data-file-title-index":index,
                                    MediaImage {state,id:id.clone(),media_epoch:*epoch,compact:true,class:"emoji-image",alt:alt.clone(),title:title.clone(),fallback}
                                }
                            }
                        },
                    }}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
