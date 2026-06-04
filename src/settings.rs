#[derive(Clone, Debug)]
pub struct Settings {
    pub ui_scale: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self { ui_scale: 1.0 }
    }
}
