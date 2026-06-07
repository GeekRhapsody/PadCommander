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
    GoBack,
    GoParent,
    UpdatePaneScrollOffset { pane: PaneSide, offset: i32 },
    Refresh,
    ToggleMarkSelected,
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
    StartDemoTask,
    StartFailingDemoTask,
    CancelActiveTask,
    OpenFavourites,
    AddFavourite,
    RemoveFavourite,
    OpenFocusedFavourite,
    FocusFavourite(usize),
    FocusDecompressOption(usize),
    IncreaseUiScale,
    DecreaseUiScale,
    OpenCommandPalette,
    OpenSettings,
    ToggleInputHelp,
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
    CursorLeft,
    CursorRight,
    MoveKeyFocus(FocusDirection),
    PressFocusedKey,
    ToggleShift,
    ToggleSymbols,
    Submit,
    Cancel,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextInputPurpose {
    CreateFolder,
    CreateFile,
    CreateTextFile,
    Rename,
    ConnectFtp,
    ArchiveName,
    ArchiveDestination,
    FtpProfileField,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FocusDirection {
    Up,
    Down,
    Left,
    Right,
}
