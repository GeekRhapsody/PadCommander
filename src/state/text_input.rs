use crate::action::{FocusDirection, TextInputAction, TextInputPurpose};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextInputDialogState {
    pub purpose: TextInputPurpose,
    pub title: String,
    pub value: String,
    pub cursor: usize,
    pub layer: KeyboardLayer,
    pub focused_row: usize,
    pub focused_col: usize,
    pub validation_error: Option<String>,
}

impl TextInputDialogState {
    pub fn new(purpose: TextInputPurpose, initial_value: String) -> Self {
        let mut dialog = Self {
            purpose,
            title: purpose.title().to_owned(),
            cursor: initial_value.chars().count(),
            value: initial_value,
            layer: KeyboardLayer::Lowercase,
            focused_row: 0,
            focused_col: 0,
            validation_error: None,
        };
        dialog.validate();
        dialog
    }

    pub fn apply_action(&mut self, action: TextInputAction) -> TextInputOutcome {
        self.validation_error = None;

        match action {
            TextInputAction::Begin { .. } | TextInputAction::Cancel => TextInputOutcome::Editing,
            TextInputAction::Insert(text) => {
                self.insert_text(&text);
                self.validate();
                TextInputOutcome::Editing
            }
            TextInputAction::Backspace => {
                self.backspace();
                self.validate();
                TextInputOutcome::Editing
            }
            TextInputAction::Clear => {
                self.value.clear();
                self.cursor = 0;
                self.validate();
                TextInputOutcome::Editing
            }
            TextInputAction::CursorLeft => {
                self.cursor = self.cursor.saturating_sub(1);
                TextInputOutcome::Editing
            }
            TextInputAction::CursorRight => {
                self.cursor = (self.cursor + 1).min(self.value.chars().count());
                TextInputOutcome::Editing
            }
            TextInputAction::MoveKeyFocus(direction) => {
                self.move_key_focus(direction);
                TextInputOutcome::Editing
            }
            TextInputAction::PressFocusedKey => self.press_focused_key(),
            TextInputAction::ToggleShift => {
                self.layer = match self.layer {
                    KeyboardLayer::Lowercase => KeyboardLayer::Uppercase,
                    KeyboardLayer::Uppercase => KeyboardLayer::Lowercase,
                    KeyboardLayer::Symbols => KeyboardLayer::Uppercase,
                };
                self.clamp_focus();
                TextInputOutcome::Editing
            }
            TextInputAction::ToggleSymbols => {
                self.layer = match self.layer {
                    KeyboardLayer::Symbols => KeyboardLayer::Lowercase,
                    _ => KeyboardLayer::Symbols,
                };
                self.clamp_focus();
                TextInputOutcome::Editing
            }
            TextInputAction::Submit => self.submit(),
        }
    }

    pub fn keys(&self) -> &'static [&'static [OnScreenKey]] {
        self.layer.keys()
    }

    pub fn focused_key(&self) -> OnScreenKey {
        let row = self.keys()[self.focused_row];
        row[self.focused_col.min(row.len() - 1)]
    }

    fn press_focused_key(&mut self) -> TextInputOutcome {
        match self.focused_key() {
            OnScreenKey::Text(text) => self.apply_action(TextInputAction::Insert(text.to_owned())),
            OnScreenKey::Space => self.apply_action(TextInputAction::Insert(" ".to_owned())),
            OnScreenKey::Backspace => self.apply_action(TextInputAction::Backspace),
            OnScreenKey::Clear => self.apply_action(TextInputAction::Clear),
            OnScreenKey::CursorLeft => self.apply_action(TextInputAction::CursorLeft),
            OnScreenKey::CursorRight => self.apply_action(TextInputAction::CursorRight),
            OnScreenKey::Shift => self.apply_action(TextInputAction::ToggleShift),
            OnScreenKey::Symbols => self.apply_action(TextInputAction::ToggleSymbols),
            OnScreenKey::Confirm => self.apply_action(TextInputAction::Submit),
            OnScreenKey::Cancel => TextInputOutcome::Cancelled,
        }
    }

    fn submit(&mut self) -> TextInputOutcome {
        self.validate();
        if let Some(error) = &self.validation_error {
            return TextInputOutcome::Blocked(error.clone());
        }

        TextInputOutcome::Submitted {
            purpose: self.purpose,
            value: self.value.trim().to_owned(),
        }
    }

    fn insert_text(&mut self, text: &str) {
        let byte_index = char_to_byte_index(&self.value, self.cursor);
        self.value.insert_str(byte_index, text);
        self.cursor += text.chars().count();
    }

    fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }

        let start = char_to_byte_index(&self.value, self.cursor - 1);
        let end = char_to_byte_index(&self.value, self.cursor);
        self.value.replace_range(start..end, "");
        self.cursor -= 1;
    }

    fn move_key_focus(&mut self, direction: FocusDirection) {
        match direction {
            FocusDirection::Up | FocusDirection::Down => {
                let old_row = self.focused_row;
                let new_row = match direction {
                    FocusDirection::Up => self.focused_row.saturating_sub(1),
                    FocusDirection::Down => (self.focused_row + 1).min(self.keys().len() - 1),
                    _ => unreachable!(),
                };
                if new_row != old_row {
                    let current_x = key_row_centers(self.keys()[old_row])[self.focused_col];
                    self.focused_col = key_row_centers(self.keys()[new_row])
                        .iter()
                        .enumerate()
                        .min_by_key(|&(_, &cx)| (cx - current_x).unsigned_abs())
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    self.focused_row = new_row;
                }
            }
            FocusDirection::Left => {
                self.focused_col = self.focused_col.saturating_sub(1);
            }
            FocusDirection::Right => {
                self.focused_col =
                    (self.focused_col + 1).min(self.keys()[self.focused_row].len() - 1);
            }
        }
        self.clamp_focus();
    }

    fn clamp_focus(&mut self) {
        self.focused_row = self.focused_row.min(self.keys().len() - 1);
        self.focused_col = self
            .focused_col
            .min(self.keys()[self.focused_row].len() - 1);
    }

    fn validate(&mut self) {
        self.validation_error = validate_text_input(self.purpose, &self.value);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyboardLayer {
    Lowercase,
    Uppercase,
    Symbols,
}

impl KeyboardLayer {
    fn keys(self) -> &'static [&'static [OnScreenKey]] {
        match self {
            KeyboardLayer::Lowercase => LOWERCASE_KEYS,
            KeyboardLayer::Uppercase => UPPERCASE_KEYS,
            KeyboardLayer::Symbols => SYMBOL_KEYS,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OnScreenKey {
    Text(&'static str),
    Space,
    Backspace,
    Clear,
    CursorLeft,
    CursorRight,
    Shift,
    Symbols,
    Confirm,
    Cancel,
}

impl OnScreenKey {
    pub fn label(self) -> &'static str {
        match self {
            OnScreenKey::Text(text) => text,
            OnScreenKey::Space => "Space",
            OnScreenKey::Backspace => "Back",
            OnScreenKey::Clear => "Clear",
            OnScreenKey::CursorLeft => "<",
            OnScreenKey::CursorRight => ">",
            OnScreenKey::Shift => "Shift",
            OnScreenKey::Symbols => "Sym",
            OnScreenKey::Confirm => "OK",
            OnScreenKey::Cancel => "Cancel",
        }
    }

    fn visual_width(self) -> u32 {
        match self {
            OnScreenKey::Space => 180,
            OnScreenKey::Backspace
            | OnScreenKey::Clear
            | OnScreenKey::Confirm
            | OnScreenKey::Cancel
            | OnScreenKey::Shift
            | OnScreenKey::Symbols => 92,
            OnScreenKey::CursorLeft | OnScreenKey::CursorRight => 56,
            OnScreenKey::Text(_) => 48,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TextInputOutcome {
    Editing,
    Cancelled,
    Blocked(String),
    Submitted {
        purpose: TextInputPurpose,
        value: String,
    },
}

impl TextInputPurpose {
    pub fn title(self) -> &'static str {
        match self {
            TextInputPurpose::CreateFolder => "Create Folder",
            TextInputPurpose::CreateFile => "Create File",
            TextInputPurpose::CreateTextFile => "Create Text File",
            TextInputPurpose::Rename => "Rename",
            TextInputPurpose::ConnectFtp => "Connect FTP",
            TextInputPurpose::ArchiveName => "Archive Name",
            TextInputPurpose::ArchiveDestination => "Extract To Folder",
            TextInputPurpose::ArchivePassword => "Archive Password",
            TextInputPurpose::ShortcutName => "Shortcut Name",
            TextInputPurpose::FtpProfileField => "FTP Profile Field",
        }
    }

    fn uses_windows_filename_validation(self) -> bool {
        matches!(
            self,
            TextInputPurpose::CreateFolder
                | TextInputPurpose::CreateFile
                | TextInputPurpose::CreateTextFile
                | TextInputPurpose::Rename
                | TextInputPurpose::ArchiveName
                | TextInputPurpose::ArchiveDestination
                | TextInputPurpose::ShortcutName
        )
    }
}

fn key_row_centers(row: &[OnScreenKey]) -> Vec<i32> {
    const SPACING: u32 = 8;
    let mut centers = Vec::with_capacity(row.len());
    let mut x = 0u32;
    for (i, key) in row.iter().enumerate() {
        if i > 0 {
            x += SPACING;
        }
        let w = key.visual_width();
        centers.push((x + w / 2) as i32);
        x += w;
    }
    // The renderer centers each row in the container, so we normalize all positions
    // relative to the row's own midpoint. This makes cross-row comparisons meaningful.
    let row_half = x as i32 / 2;
    centers.iter().map(|&c| c - row_half).collect()
}

const LOWERCASE_KEYS: &[&[OnScreenKey]] = &[
    &[
        OnScreenKey::Text("q"),
        OnScreenKey::Text("w"),
        OnScreenKey::Text("e"),
        OnScreenKey::Text("r"),
        OnScreenKey::Text("t"),
        OnScreenKey::Text("y"),
        OnScreenKey::Text("u"),
        OnScreenKey::Text("i"),
        OnScreenKey::Text("o"),
        OnScreenKey::Text("p"),
        OnScreenKey::Backspace,
    ],
    &[
        OnScreenKey::Text("a"),
        OnScreenKey::Text("s"),
        OnScreenKey::Text("d"),
        OnScreenKey::Text("f"),
        OnScreenKey::Text("g"),
        OnScreenKey::Text("h"),
        OnScreenKey::Text("j"),
        OnScreenKey::Text("k"),
        OnScreenKey::Text("l"),
    ],
    &[
        OnScreenKey::Shift,
        OnScreenKey::Text("z"),
        OnScreenKey::Text("x"),
        OnScreenKey::Text("c"),
        OnScreenKey::Text("v"),
        OnScreenKey::Text("b"),
        OnScreenKey::Text("n"),
        OnScreenKey::Text("m"),
        OnScreenKey::Clear,
    ],
    &[
        OnScreenKey::Symbols,
        OnScreenKey::CursorLeft,
        OnScreenKey::Space,
        OnScreenKey::CursorRight,
        OnScreenKey::Cancel,
        OnScreenKey::Confirm,
    ],
];

const UPPERCASE_KEYS: &[&[OnScreenKey]] = &[
    &[
        OnScreenKey::Text("Q"),
        OnScreenKey::Text("W"),
        OnScreenKey::Text("E"),
        OnScreenKey::Text("R"),
        OnScreenKey::Text("T"),
        OnScreenKey::Text("Y"),
        OnScreenKey::Text("U"),
        OnScreenKey::Text("I"),
        OnScreenKey::Text("O"),
        OnScreenKey::Text("P"),
        OnScreenKey::Backspace,
    ],
    &[
        OnScreenKey::Text("A"),
        OnScreenKey::Text("S"),
        OnScreenKey::Text("D"),
        OnScreenKey::Text("F"),
        OnScreenKey::Text("G"),
        OnScreenKey::Text("H"),
        OnScreenKey::Text("J"),
        OnScreenKey::Text("K"),
        OnScreenKey::Text("L"),
    ],
    &[
        OnScreenKey::Shift,
        OnScreenKey::Text("Z"),
        OnScreenKey::Text("X"),
        OnScreenKey::Text("C"),
        OnScreenKey::Text("V"),
        OnScreenKey::Text("B"),
        OnScreenKey::Text("N"),
        OnScreenKey::Text("M"),
        OnScreenKey::Clear,
    ],
    LOWERCASE_KEYS[3],
];

const SYMBOL_KEYS: &[&[OnScreenKey]] = &[
    &[
        OnScreenKey::Text("1"),
        OnScreenKey::Text("2"),
        OnScreenKey::Text("3"),
        OnScreenKey::Text("4"),
        OnScreenKey::Text("5"),
        OnScreenKey::Text("6"),
        OnScreenKey::Text("7"),
        OnScreenKey::Text("8"),
        OnScreenKey::Text("9"),
        OnScreenKey::Text("0"),
        OnScreenKey::Backspace,
    ],
    &[
        OnScreenKey::Text("-"),
        OnScreenKey::Text("_"),
        OnScreenKey::Text("."),
        OnScreenKey::Text(","),
        OnScreenKey::Text(":"),
        OnScreenKey::Text("/"),
        OnScreenKey::Text("\\"),
        OnScreenKey::Text("("),
        OnScreenKey::Text(")"),
    ],
    &[
        OnScreenKey::Shift,
        OnScreenKey::Text("#"),
        OnScreenKey::Text("@"),
        OnScreenKey::Text("+"),
        OnScreenKey::Text("="),
        OnScreenKey::Text("!"),
        OnScreenKey::Text("&"),
        OnScreenKey::Text(";"),
        OnScreenKey::Clear,
    ],
    LOWERCASE_KEYS[3],
];

fn validate_text_input(purpose: TextInputPurpose, value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Some("Name cannot be empty.".to_owned());
    }

    if purpose.uses_windows_filename_validation() {
        validate_windows_filename(trimmed)
    } else {
        None
    }
}

fn validate_windows_filename(value: &str) -> Option<String> {
    const INVALID_CHARS: [char; 9] = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
    if let Some(ch) = value.chars().find(|ch| INVALID_CHARS.contains(ch)) {
        return Some(format!("Windows filenames cannot contain '{ch}'."));
    }

    if value.ends_with('.') || value.ends_with(' ') {
        return Some("Windows filenames cannot end with a space or period.".to_owned());
    }

    let stem = value
        .split('.')
        .next()
        .unwrap_or(value)
        .to_ascii_uppercase();
    if matches!(
        stem.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    ) {
        return Some(format!("'{value}' is a reserved Windows filename."));
    }

    None
}

fn char_to_byte_index(value: &str, char_index: usize) -> usize {
    value
        .char_indices()
        .nth(char_index)
        .map(|(index, _)| index)
        .unwrap_or(value.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_dialog_inserts_at_cursor_and_backspaces() {
        let mut dialog = TextInputDialogState::new(TextInputPurpose::CreateFile, "ab".to_owned());

        dialog.apply_action(TextInputAction::CursorLeft);
        dialog.apply_action(TextInputAction::Insert("X".to_owned()));
        assert_eq!(dialog.value, "aXb");

        dialog.apply_action(TextInputAction::Backspace);
        assert_eq!(dialog.value, "ab");
    }

    #[test]
    fn text_dialog_blocks_windows_invalid_filename_characters() {
        let mut dialog = TextInputDialogState::new(TextInputPurpose::CreateFolder, String::new());
        dialog.apply_action(TextInputAction::Insert("bad:name".to_owned()));

        let outcome = dialog.apply_action(TextInputAction::Submit);

        assert!(matches!(outcome, TextInputOutcome::Blocked(_)));
        assert!(dialog.validation_error.is_some());
    }

    #[test]
    fn text_dialog_submits_valid_filename() {
        let mut dialog = TextInputDialogState::new(TextInputPurpose::Rename, "notes".to_owned());

        let outcome = dialog.apply_action(TextInputAction::Submit);

        assert_eq!(
            outcome,
            TextInputOutcome::Submitted {
                purpose: TextInputPurpose::Rename,
                value: "notes".to_owned()
            }
        );
    }
}
