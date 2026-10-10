//! Renderer-owned huddle stage preferences and device projections.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct HuddleDevice {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct HuddleDevices {
    #[serde(default)]
    pub microphone: Vec<HuddleDevice>,
    #[serde(default)]
    pub camera: Vec<HuddleDevice>,
    #[serde(default)]
    pub speaker: Vec<HuddleDevice>,
    #[serde(default)]
    pub output_supported: bool,
    #[serde(default)]
    pub screen_supported: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct HuddleRect {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

impl HuddleRect {
    pub fn valid(self) -> bool {
        [self.left, self.top, self.width, self.height]
            .iter()
            .all(|n| n.is_finite())
            && self.left >= 0.0
            && self.top >= 0.0
            && self.width >= 200.0
            && self.height >= 150.0
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct HuddleCaption {
    pub id: String,
    pub user: Option<String>,
    pub text: String,
    pub partial: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HuddleReaction {
    #[serde(skip, default = "std::time::Instant::now")]
    pub shown_at: std::time::Instant,
    pub user: Option<String>,
    pub emoji: String,
}

pub struct HuddleUi {
    pub generation: u64,
    pub visible: bool,
    pub expanded: bool,
    pub settings: bool,
    pub invite: bool,
    pub invite_query: String,
    pub invite_pending: bool,
    pub rect: Option<HuddleRect>,
    pub devices: HuddleDevices,
    pub microphone: String,
    pub camera: String,
    pub speaker: String,
    pub device_pending: bool,
    pub background: String,
    pub captions_on: bool,
    pub captions_pending: bool,
    pub captions: Vec<HuddleCaption>,
    pub reactions: Vec<HuddleReaction>,
}

impl Default for HuddleUi {
    fn default() -> Self {
        Self {
            generation: 0,
            visible: true,
            expanded: false,
            settings: false,
            invite: false,
            invite_query: String::new(),
            invite_pending: false,
            rect: None,
            devices: HuddleDevices::default(),
            microphone: String::new(),
            camera: String::new(),
            speaker: String::new(),
            device_pending: false,
            background: "none".into(),
            captions_on: false,
            captions_pending: false,
            captions: Vec::new(),
            reactions: Vec::new(),
        }
    }
}

impl HuddleUi {
    pub fn begin(&mut self, generation: u64) {
        let rect = self.rect;
        *self = Self {
            generation,
            rect,
            ..Self::default()
        };
    }

    pub fn caption(&mut self, caption: HuddleCaption) {
        if caption.text.is_empty() {
            return;
        }
        if let Some(previous) = self.captions.iter_mut().find(|row| row.id == caption.id) {
            *previous = caption;
        } else {
            self.captions.push(caption);
        }
        let excess = self.captions.len().saturating_sub(20);
        self.captions.drain(..excess);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn partial_captions_replace_and_calls_reset_ephemeral_state() {
        let mut ui = HuddleUi::default();
        for partial in [true, false] {
            ui.caption(HuddleCaption {
                id: "1".into(),
                user: None,
                text: "Hello".into(),
                partial,
            });
        }
        assert_eq!(ui.captions.len(), 1);
        assert!(!ui.captions[0].partial);
        for n in 2..40 {
            ui.caption(HuddleCaption {
                id: n.to_string(),
                user: None,
                text: "Hello".into(),
                partial: false,
            });
        }
        assert_eq!(ui.captions.len(), 20);
        ui.begin(2);
        assert!(ui.captions.is_empty());
        assert!(!ui.captions_on);
    }
    #[test]
    fn invalid_geometry_is_rejected() {
        assert!(
            !HuddleRect {
                left: f64::NAN,
                top: 20.0,
                width: 400.0,
                height: 300.0
            }
            .valid()
        );
        assert!(
            !HuddleRect {
                left: 20.0,
                top: 20.0,
                width: -400.0,
                height: 300.0
            }
            .valid()
        );
    }
}
