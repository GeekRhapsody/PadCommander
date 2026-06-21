use serde::{Deserialize, Serialize};
use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

pub const MIN_UI_SCALE: f32 = 0.75;
pub const MAX_UI_SCALE: f32 = 2.0;
pub const UI_SCALE_STEP: f32 = 0.1;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ControllerOverride {
    #[default]
    Auto,
    Xbox,
    PlayStation4,
    PlayStation5,
    NintendoSwitch,
}

impl ControllerOverride {
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Xbox => "Xbox",
            Self::PlayStation4 => "PS4",
            Self::PlayStation5 => "PS5",
            Self::NintendoSwitch => "Nintendo",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Auto => Self::Xbox,
            Self::Xbox => Self::PlayStation4,
            Self::PlayStation4 => Self::PlayStation5,
            Self::PlayStation5 => Self::NintendoSwitch,
            Self::NintendoSwitch => Self::Auto,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Auto => Self::NintendoSwitch,
            Self::Xbox => Self::Auto,
            Self::PlayStation4 => Self::Xbox,
            Self::PlayStation5 => Self::PlayStation4,
            Self::NintendoSwitch => Self::PlayStation5,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub ui_scale: f32,
    #[serde(default = "default_use_recycle_bin")]
    pub use_recycle_bin: bool,
    #[serde(default)]
    pub show_hidden_files: bool,
    #[serde(default)]
    pub controller_override: ControllerOverride,
    #[serde(default)]
    pub favourites: Vec<String>,
    #[serde(default)]
    pub last_left_pane_location: Option<String>,
    #[serde(default)]
    pub last_right_pane_location: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ui_scale: 1.0,
            use_recycle_bin: true,
            show_hidden_files: false,
            controller_override: ControllerOverride::Auto,
            favourites: Vec::new(),
            last_left_pane_location: None,
            last_right_pane_location: None,
        }
    }
}

impl Settings {
    pub fn sanitize(mut self) -> Self {
        self.ui_scale = clamp_ui_scale(self.ui_scale);
        self.favourites.sort();
        self.favourites.dedup();
        self
    }
}

fn default_use_recycle_bin() -> bool {
    true
}

pub fn clamp_ui_scale(ui_scale: f32) -> f32 {
    if ui_scale.is_finite() {
        ui_scale.clamp(MIN_UI_SCALE, MAX_UI_SCALE)
    } else {
        Settings::default().ui_scale
    }
}

pub fn config_file_path() -> PathBuf {
    config_dir().join("settings.json")
}

pub fn load() -> io::Result<Settings> {
    load_from_path(&config_file_path())
}

pub fn load_from_path(path: &Path) -> io::Result<Settings> {
    match fs::read_to_string(path) {
        Ok(contents) => serde_json::from_str::<Settings>(&contents)
            .map(Settings::sanitize)
            .map_err(|source| io::Error::new(io::ErrorKind::InvalidData, source)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Settings::default()),
        Err(error) => Err(error),
    }
}

pub fn save_to_path(settings: &Settings, path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let contents =
        serde_json::to_string_pretty(&settings.clone().sanitize()).map_err(io::Error::other)?;
    fs::write(path, contents)
}

fn config_dir() -> PathBuf {
    env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir)
        .join("PadCommander")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_serializes_all_persistent_fields() {
        let settings = Settings {
            ui_scale: 1.4,
            use_recycle_bin: false,
            show_hidden_files: true,
            controller_override: ControllerOverride::PlayStation5,
            favourites: vec!["C:\\Games".to_owned(), "D:\\Downloads".to_owned()],
            last_left_pane_location: Some("C:\\Users".to_owned()),
            last_right_pane_location: Some("D:\\".to_owned()),
        };

        let json = serde_json::to_string(&settings).expect("serialize settings");
        let restored: Settings = serde_json::from_str(&json).expect("deserialize settings");

        assert_eq!(restored, settings);
    }

    #[test]
    fn missing_new_fields_deserialize_to_defaults() {
        let settings: Settings =
            serde_json::from_str(r#"{"ui_scale":1.25}"#).expect("deserialize old settings");

        assert_eq!(settings.ui_scale, 1.25);
        assert!(settings.use_recycle_bin);
        assert!(!settings.show_hidden_files);
        assert!(settings.favourites.is_empty());
        assert_eq!(settings.last_left_pane_location, None);
        assert_eq!(settings.last_right_pane_location, None);
    }

    #[test]
    fn sanitize_clamps_scale_and_deduplicates_favourites() {
        let settings = Settings {
            ui_scale: 99.0,
            use_recycle_bin: true,
            show_hidden_files: false,
            favourites: vec!["B".to_owned(), "A".to_owned(), "A".to_owned()],
            last_left_pane_location: None,
            last_right_pane_location: None,
        }
        .sanitize();

        assert_eq!(settings.ui_scale, MAX_UI_SCALE);
        assert_eq!(settings.favourites, ["A", "B"]);
    }
}
