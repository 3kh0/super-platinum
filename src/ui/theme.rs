use std::sync::{LazyLock, RwLock};

use iced::theme::palette::Seed;
use iced::widget::{button, container, scrollable, slider, text_input, toggler as toggler_widget};
use iced::{Background, Border, Color, Element, Length, Shadow, Theme, Vector};

use crate::config::{ColorRole, HexColor, RoleColorOverrides, Settings, ThemePreset};

mod base;
mod navigation;
mod styles;

use base::vars;
pub use base::*;
pub use navigation::*;
pub use styles::*;

#[cfg(test)]
use navigation::{scrollbar_interaction, scrollbar_state};
#[cfg(test)]
mod tests;
