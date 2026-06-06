# Testing Notes

## Create EXE Shortcut

Manual Windows test:

1. Open Handheld Commander on Windows.
2. Navigate one pane to a folder containing an `.exe` file.
3. Navigate the other pane to a writable folder.
4. Select the `.exe` file and invoke `Shortcut`.
5. Confirm that `[exe file stem].lnk` appears in the other pane.
6. Open the shortcut in Windows Explorer and verify it launches the selected EXE.
7. Repeat the action with the same target and verify the overwrite/cancel prompt is usable with gamepad or keyboard.
