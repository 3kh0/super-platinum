use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AccentColor {
    #[default]
    Blue,
    Red,
    Green,
    Yellow,
    Purple,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ThemePreset {
    #[default]
    Countertop,
    BlueSteel,
    PaperBag,
}

impl ThemePreset {
    pub const ALL: [Self; 3] = [Self::Countertop, Self::BlueSteel, Self::PaperBag];

    pub fn label(self) -> &'static str {
        match self {
            Self::Countertop => "Jet Black",
            Self::BlueSteel => "Midnight Blue",
            Self::PaperBag => "Obsidian",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Countertop => "Neutral black with electric blue",
            Self::BlueSteel => "Blue-black with cool highlights",
            Self::PaperBag => "Pure black with crisp blue contrast",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HexColor([u8; 3]);

impl HexColor {
    pub const fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self([r, g, b])
    }

    pub const fn rgb(self) -> [u8; 3] {
        self.0
    }

    pub fn as_hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.0[0], self.0[1], self.0[2])
    }
}

impl TryFrom<String> for HexColor {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let hex = value.strip_prefix('#').unwrap_or(&value);
        if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("expected a color like #E8875B".to_owned());
        }
        let component = |range| {
            u8::from_str_radix(&hex[range], 16)
                .map_err(|_| "expected a color like #E8875B".to_owned())
        };
        Ok(Self([component(0..2)?, component(2..4)?, component(4..6)?]))
    }
}

impl From<HexColor> for String {
    fn from(value: HexColor) -> Self {
        value.as_hex()
    }
}

impl Serialize for HexColor {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.as_hex())
    }
}

impl<'de> Deserialize<'de> for HexColor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .try_into()
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RoleColorOverrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary: Option<HexColor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hover: Option<HexColor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mention: Option<HexColor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub success: Option<HexColor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning: Option<HexColor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub danger: Option<HexColor>,
}

impl RoleColorOverrides {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    pub fn get(self, role: ColorRole) -> Option<HexColor> {
        match role {
            ColorRole::Primary => self.primary,
            ColorRole::Hover => self.hover,
            ColorRole::Mention => self.mention,
            ColorRole::Success => self.success,
            ColorRole::Warning => self.warning,
            ColorRole::Danger => self.danger,
        }
    }

    pub fn set(&mut self, role: ColorRole, value: Option<HexColor>) {
        match role {
            ColorRole::Primary => self.primary = value,
            ColorRole::Hover => self.hover = value,
            ColorRole::Mention => self.mention = value,
            ColorRole::Success => self.success = value,
            ColorRole::Warning => self.warning = value,
            ColorRole::Danger => self.danger = value,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorRole {
    Primary,
    Hover,
    Mention,
    Success,
    Warning,
    Danger,
}

impl ColorRole {
    pub const ALL: [Self; 6] = [
        Self::Primary,
        Self::Hover,
        Self::Mention,
        Self::Success,
        Self::Warning,
        Self::Danger,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Primary => "Primary",
            Self::Hover => "Hover",
            Self::Mention => "Mention",
            Self::Success => "Success",
            Self::Warning => "Warning",
            Self::Danger => "Danger",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum BackgroundFit {
    #[default]
    Cover,
    Contain,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackgroundSettings {
    pub file_name: String,
    #[serde(default)]
    pub fit: BackgroundFit,
    #[serde(default = "default_background_dim")]
    pub dim: f32,
    #[serde(default = "default_surface_opacity")]
    pub surface_opacity: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub preset: ThemePreset,
    #[serde(default)]
    pub colors: RoleColorOverrides,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<BackgroundSettings>,
    /// Read old settings, but never write the superseded accent field.
    #[serde(default, rename = "accent", skip_serializing)]
    pub(super) legacy_accent: Option<AccentColor>,
    #[serde(default = "default_gap")]
    pub gap: f32,
    #[serde(default = "default_panel_radius")]
    pub panel_radius: f32,
    /// Border thickness around the main panels (`--border-thickness`).
    #[serde(default = "default_border_thickness")]
    pub border_thickness: f32,
    /// Channel sidebar width in px (user-draggable).
    #[serde(default = "default_sidebar_width")]
    pub sidebar_width: f32,
}

fn default_gap() -> f32 {
    8.0
}
fn default_panel_radius() -> f32 {
    8.0
}
fn default_border_thickness() -> f32 {
    1.0
}
fn default_sidebar_width() -> f32 {
    240.0
}
fn default_background_dim() -> f32 {
    0.45
}
fn default_surface_opacity() -> f32 {
    0.88
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            preset: ThemePreset::default(),
            colors: RoleColorOverrides::default(),
            background: None,
            legacy_accent: None,
            gap: default_gap(),
            panel_radius: default_panel_radius(),
            border_thickness: default_border_thickness(),
            sidebar_width: default_sidebar_width(),
        }
    }
}

/// Clamp range for the draggable sidebar width.
pub const SIDEBAR_WIDTH_MIN: f32 = 180.0;
pub const SIDEBAR_WIDTH_MAX: f32 = 520.0;

pub(super) fn settings_path() -> Result<PathBuf, AppError> {
    Ok(config_dir()?.join("settings.json"))
}

/// Load appearance settings, falling back to defaults on any error.
pub fn load_settings() -> Settings {
    let Ok(path) = settings_path() else {
        return Settings::default();
    };
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(migrate_legacy_accent)
            .unwrap_or_default(),
        Err(_) => Settings::default(),
    }
}

fn migrate_legacy_accent(mut settings: Settings) -> Settings {
    if settings.colors.primary.is_none()
        && let Some(accent) = settings.legacy_accent.take()
    {
        let (primary, hover) = match accent {
            AccentColor::Blue => ("#4AAFE8", "#72C5F0"),
            AccentColor::Red => ("#D96868", "#E98989"),
            AccentColor::Green => ("#54B88A", "#7BCBA5"),
            AccentColor::Yellow => ("#D6A93D", "#E5C367"),
            AccentColor::Purple => ("#B382DA", "#C9A4E6"),
        };
        settings.colors.primary = primary.to_owned().try_into().ok();
        settings.colors.hover = hover.to_owned().try_into().ok();
    }
    settings
}

pub fn save_settings(settings: &Settings) -> Result<(), AppError> {
    let dir = config_dir()?;
    std::fs::create_dir_all(&dir)?;
    let json = serde_json::to_string_pretty(settings)?;
    std::fs::write(settings_path()?, json)?;
    Ok(())
}

pub fn background_dir() -> Result<PathBuf, AppError> {
    Ok(config_dir()?.join("backgrounds"))
}

pub fn background_path(settings: &BackgroundSettings) -> Option<PathBuf> {
    let name = Path::new(&settings.file_name);
    if name.components().count() != 1 {
        return None;
    }
    background_dir().ok().map(|dir| dir.join(name))
}
