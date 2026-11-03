use eframe::egui;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ControllerType {
    #[default]
    Xbox,
    PlayStation4,
    PlayStation5,
    NintendoSwitch,
}

impl ControllerType {
    pub fn from_gamepad_name(name: &str) -> Self {
        let lower = name.to_lowercase();
        if lower.contains("dualsense") || lower.contains("ps5") {
            Self::PlayStation5
        } else if lower.contains("dualshock") || lower.contains("ps4") {
            Self::PlayStation4
        } else if lower.contains("nintendo") || lower.contains("pro controller") {
            Self::NintendoSwitch
        } else {
            Self::Xbox
        }
    }
}

#[derive(Clone, Copy)]
pub struct Btn {
    pub bytes: &'static [u8],
    pub uri: &'static str,
}

impl Btn {
    const fn new(bytes: &'static [u8], uri: &'static str) -> Self {
        Self { bytes, uri }
    }
}

#[derive(Clone, Copy)]
pub struct ButtonImages {
    pub south: Btn,
    pub east: Btn,
    pub west: Btn,
    pub north: Btn,
    pub lb: Btn,
    pub rb: Btn,
    pub dpad_up: Btn,
    pub dpad_down: Btn,
    pub dpad_left: Btn,
    pub dpad_right: Btn,
    pub dpad_all: Btn,
    pub l3: Btn,
    pub start: Btn,
    pub select: Btn,
}

impl ButtonImages {
    pub fn for_controller(ct: ControllerType) -> Self {
        match ct {
            ControllerType::Xbox => Self {
                south: Btn::new(
                    include_bytes!("../../assets/buttons/xbox/T_X_A_White_Alt.png"),
                    "b://x_south",
                ),
                east: Btn::new(
                    include_bytes!("../../assets/buttons/xbox/T_X_B_White_Alt.png"),
                    "b://x_east",
                ),
                west: Btn::new(
                    include_bytes!("../../assets/buttons/xbox/T_X_X_White_Alt.png"),
                    "b://x_west",
                ),
                north: Btn::new(
                    include_bytes!("../../assets/buttons/xbox/T_X_Y_White_Alt.png"),
                    "b://x_north",
                ),
                lb: Btn::new(
                    include_bytes!("../../assets/buttons/xbox/T_X_LB_Alt.png"),
                    "b://x_lb",
                ),
                rb: Btn::new(
                    include_bytes!("../../assets/buttons/xbox/T_X_RB_Alt.png"),
                    "b://x_rb",
                ),
                dpad_up: Btn::new(
                    include_bytes!("../../assets/buttons/xbox/T_X_Dpad_Up_Alt.png"),
                    "b://x_dup",
                ),
                dpad_down: Btn::new(
                    include_bytes!("../../assets/buttons/xbox/T_X_Dpad_Down_Alt.png"),
                    "b://x_ddown",
                ),
                dpad_left: Btn::new(
                    include_bytes!("../../assets/buttons/xbox/T_X_Dpad_Left_Alt.png"),
                    "b://x_dleft",
                ),
                dpad_right: Btn::new(
                    include_bytes!("../../assets/buttons/xbox/T_X_Dpad_Right_Alt.png"),
                    "b://x_dright",
                ),
                dpad_all: Btn::new(
                    include_bytes!("../../assets/buttons/xbox/T_X_Dpad_Alt.png"),
                    "b://x_dall",
                ),
                l3: Btn::new(
                    include_bytes!("../../assets/buttons/xbox/T_X_Left_Stick_Click_Alt.png"),
                    "b://x_l3",
                ),
                start: Btn::new(
                    include_bytes!("../../assets/buttons/xbox/T_X_X_Alt.png"),
                    "b://x_start",
                ),
                select: Btn::new(
                    include_bytes!("../../assets/buttons/xbox/T_X_Share_Alt.png"),
                    "b://x_select",
                ),
            },
            ControllerType::PlayStation4 => Self {
                south: Btn::new(
                    include_bytes!("../../assets/buttons/ps4/T_P4_Cross.png"),
                    "b://p4_south",
                ),
                east: Btn::new(
                    include_bytes!("../../assets/buttons/ps4/T_P4_Circle.png"),
                    "b://p4_east",
                ),
                west: Btn::new(
                    include_bytes!("../../assets/buttons/ps4/T_P4_Square.png"),
                    "b://p4_west",
                ),
                north: Btn::new(
                    include_bytes!("../../assets/buttons/ps4/T_P4_Triangle.png"),
                    "b://p4_north",
                ),
                lb: Btn::new(
                    include_bytes!("../../assets/buttons/ps4/T_P4_L1.png"),
                    "b://p4_lb",
                ),
                rb: Btn::new(
                    include_bytes!("../../assets/buttons/ps4/T_P4_R1.png"),
                    "b://p4_rb",
                ),
                dpad_up: Btn::new(
                    include_bytes!("../../assets/buttons/ps4/T_P4_Dpad_UP.png"),
                    "b://p4_dup",
                ),
                dpad_down: Btn::new(
                    include_bytes!("../../assets/buttons/ps4/T_P4_Dpad_Down.png"),
                    "b://p4_ddown",
                ),
                dpad_left: Btn::new(
                    include_bytes!("../../assets/buttons/ps4/T_P4_Dpad_Left.png"),
                    "b://p4_dleft",
                ),
                dpad_right: Btn::new(
                    include_bytes!("../../assets/buttons/ps4/T_P4_Dpad_Right.png"),
                    "b://p4_dright",
                ),
                dpad_all: Btn::new(
                    include_bytes!("../../assets/buttons/ps4/T_P4_Dpad.png"),
                    "b://p4_dall",
                ),
                l3: Btn::new(
                    include_bytes!("../../assets/buttons/ps4/T_P4_L3.png"),
                    "b://p4_l3",
                ),
                start: Btn::new(
                    include_bytes!("../../assets/buttons/ps4/T_P4_Options.png"),
                    "b://p4_start",
                ),
                select: Btn::new(
                    include_bytes!("../../assets/buttons/ps4/T_P4_Share.png"),
                    "b://p4_select",
                ),
            },
            ControllerType::PlayStation5 => Self {
                south: Btn::new(
                    include_bytes!("../../assets/buttons/ps5/T_P5_Cross_Alt.png"),
                    "b://p5_south",
                ),
                east: Btn::new(
                    include_bytes!("../../assets/buttons/ps5/T_P5_Circle_Alt.png"),
                    "b://p5_east",
                ),
                west: Btn::new(
                    include_bytes!("../../assets/buttons/ps5/T_P5_Square_Alt.png"),
                    "b://p5_west",
                ),
                north: Btn::new(
                    include_bytes!("../../assets/buttons/ps5/T_P5_Triangle_Alt.png"),
                    "b://p5_north",
                ),
                lb: Btn::new(
                    include_bytes!("../../assets/buttons/ps5/T_P5_L1_Alt.png"),
                    "b://p5_lb",
                ),
                rb: Btn::new(
                    include_bytes!("../../assets/buttons/ps5/T_P5_R1_Alt.png"),
                    "b://p5_rb",
                ),
                dpad_up: Btn::new(
                    include_bytes!("../../assets/buttons/ps5/T_P5_Dpad_UP_Alt.png"),
                    "b://p5_dup",
                ),
                dpad_down: Btn::new(
                    include_bytes!("../../assets/buttons/ps5/T_P5_Dpad_Down_Alt.png"),
                    "b://p5_ddown",
                ),
                dpad_left: Btn::new(
                    include_bytes!("../../assets/buttons/ps5/T_P5_Dpad_Left_Alt.png"),
                    "b://p5_dleft",
                ),
                dpad_right: Btn::new(
                    include_bytes!("../../assets/buttons/ps5/T_P5_Dpad_Right_Alt.png"),
                    "b://p5_dright",
                ),
                dpad_all: Btn::new(
                    include_bytes!("../../assets/buttons/ps5/T_P5_Dpad_Alt.png"),
                    "b://p5_dall",
                ),
                l3: Btn::new(
                    include_bytes!("../../assets/buttons/ps5/T_P5_L3_Alt.png"),
                    "b://p5_l3",
                ),
                start: Btn::new(
                    include_bytes!("../../assets/buttons/ps5/T_P5_Options_Alt.png"),
                    "b://p5_start",
                ),
                select: Btn::new(
                    include_bytes!("../../assets/buttons/ps5/T_P5_Share_Alt.png"),
                    "b://p5_select",
                ),
            },
            ControllerType::NintendoSwitch => Self {
                south: Btn::new(
                    include_bytes!("../../assets/buttons/nintendo/T_S_A_Alt.png"),
                    "b://ns_south",
                ),
                east: Btn::new(
                    include_bytes!("../../assets/buttons/nintendo/T_S_B_Alt.png"),
                    "b://ns_east",
                ),
                west: Btn::new(
                    include_bytes!("../../assets/buttons/nintendo/T_S_X_Alt.png"),
                    "b://ns_west",
                ),
                north: Btn::new(
                    include_bytes!("../../assets/buttons/nintendo/T_S_Y_Alt.png"),
                    "b://ns_north",
                ),
                lb: Btn::new(
                    include_bytes!("../../assets/buttons/nintendo/T_S_LB_Alt.png"),
                    "b://ns_lb",
                ),
                rb: Btn::new(
                    include_bytes!("../../assets/buttons/nintendo/T_S_RB_Alt.png"),
                    "b://ns_rb",
                ),
                dpad_up: Btn::new(
                    include_bytes!("../../assets/buttons/nintendo/T_S_Dpad_Up_Alt.png"),
                    "b://ns_dup",
                ),
                dpad_down: Btn::new(
                    include_bytes!("../../assets/buttons/nintendo/T_S_Dpad_Down_Alt.png"),
                    "b://ns_ddown",
                ),
                dpad_left: Btn::new(
                    include_bytes!("../../assets/buttons/nintendo/T_S_Dpad_Left_Alt.png"),
                    "b://ns_dleft",
                ),
                dpad_right: Btn::new(
                    include_bytes!("../../assets/buttons/nintendo/T_S_Dpad_Right_Alt.png"),
                    "b://ns_dright",
                ),
                dpad_all: Btn::new(
                    include_bytes!("../../assets/buttons/nintendo/T_S_Dpad_Alt.png"),
                    "b://ns_dall",
                ),
                l3: Btn::new(
                    include_bytes!("../../assets/buttons/nintendo/T_S_L_Alt.png"),
                    "b://ns_l3",
                ),
                start: Btn::new(
                    include_bytes!("../../assets/buttons/nintendo/T_S_Plus_Alt.png"),
                    "b://ns_start",
                ),
                select: Btn::new(
                    include_bytes!("../../assets/buttons/nintendo/T_S_Minus_Alt.png"),
                    "b://ns_select",
                ),
            },
        }
    }
}

pub fn btn(ui: &mut egui::Ui, b: Btn) {
    btn_sized(ui, b, 34.0);
}

pub fn btn_sized(ui: &mut egui::Ui, b: Btn, size: f32) {
    ui.add(egui::Image::from_bytes(b.uri, b.bytes).fit_to_exact_size(egui::vec2(size, size)));
}
