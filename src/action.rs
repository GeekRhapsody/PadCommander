use crate::state::PaneSide;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    ActivatePane(PaneSide),
    SwitchPane,
    PlaceholderCommand(&'static str),
    Quit,
}
