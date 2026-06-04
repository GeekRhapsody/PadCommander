use crate::state::PaneSide;

#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommanderAction {
    ActivatePane(PaneSide),
    SelectEntry { pane: PaneSide, index: usize },
    SwitchPane,
    MoveSelectionUp,
    MoveSelectionDown,
    MoveSelectionPageUp,
    MoveSelectionPageDown,
    MoveSelectionHome,
    MoveSelectionEnd,
    OpenSelected,
    GoParent,
    Refresh,
    CreateFolder,
    CreateFile,
    CreateTextFile,
    Rename,
    Delete,
    CopyToOtherPane,
    MoveToOtherPane,
    CompressToOtherPane,
    DecompressToOtherPane,
    ConnectFtp,
    UploadToOtherPane,
    DownloadToOtherPane,
    CreateShortcutInOtherPane,
    OpenFavourites,
    AddFavourite,
    OpenSettings,
    Confirm,
    Cancel,
    TextInput(TextInputAction),
    QuitDevelopment,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TextInputAction {
    Begin {
        purpose: TextInputPurpose,
        initial_value: String,
    },
    Insert(String),
    Backspace,
    Clear,
    Submit,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextInputPurpose {
    CreateFolder,
    CreateFile,
    CreateTextFile,
    Rename,
    ConnectFtp,
}
