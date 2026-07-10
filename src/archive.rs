use std::{
    collections::HashSet,
    env,
    ffi::OsString,
    fs,
    io::{self, BufRead, Read, Write},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

use unarc_rs::unified::{ArchiveFormat as UnarcFormat, ArchiveOptions};

use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

use crate::tasks::{ProgressSnapshot, TaskCompletionData, TaskOutcome, TaskReporter};

const COMMAND_POLL_INTERVAL: Duration = Duration::from_millis(50);

pub trait ArchiveService {
    fn create_archive(
        &self,
        reporter: TaskReporter,
        sources: Vec<PathBuf>,
        archive_path: PathBuf,
    ) -> TaskOutcome;

    fn extract_archive(
        &self,
        reporter: TaskReporter,
        archive_path: PathBuf,
        destination: PathBuf,
        password: Option<String>,
    ) -> TaskOutcome;
}

#[derive(Clone, Debug, Default)]
pub struct LocalArchiveService;

impl ArchiveService for LocalArchiveService {
    fn create_archive(
        &self,
        reporter: TaskReporter,
        sources: Vec<PathBuf>,
        archive_path: PathBuf,
    ) -> TaskOutcome {
        let format = match ArchiveFormat::from_path(&archive_path) {
            Ok(format) => format,
            Err(error) => return TaskOutcome::Failed(error),
        };

        match format {
            ArchiveFormat::Zip => zip_create_archive(reporter, sources, archive_path),
            ArchiveFormat::SevenZip => external_create_archive(
                reporter,
                ExternalArchiveFormat::SevenZip,
                sources,
                archive_path,
            ),
            ArchiveFormat::Rar => {
                external_create_archive(reporter, ExternalArchiveFormat::Rar, sources, archive_path)
            }
        }
    }

    fn extract_archive(
        &self,
        reporter: TaskReporter,
        archive_path: PathBuf,
        destination: PathBuf,
        password: Option<String>,
    ) -> TaskOutcome {
        match ArchiveFormat::from_path(&archive_path) {
            Ok(ArchiveFormat::Zip) => {
                unarc_extract_archive(reporter, archive_path, destination, password)
            }
            Ok(ArchiveFormat::SevenZip | ArchiveFormat::Rar) => {
                match find_command(&["7z", "7za", "7zz"]) {
                    Some(tool) => external_extract_archive(
                        reporter,
                        tool,
                        archive_path,
                        destination,
                        password,
                    ),
                    None => unarc_extract_archive(reporter, archive_path, destination, password),
                }
            }
            Err(error) => TaskOutcome::Failed(error),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ArchiveFormat {
    Zip,
    Rar,
    SevenZip,
}

impl ArchiveFormat {
    fn from_path(path: &Path) -> Result<Self, String> {
        match path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref()
        {
            Some("zip") => Ok(Self::Zip),
            Some("rar") => Ok(Self::Rar),
            Some("7z") => Ok(Self::SevenZip),
            Some(extension) => Err(format!(
                "Unsupported archive type .{extension}. Supported types are .zip, .rar, and .7z."
            )),
            None => Err("Archive name must end in .zip, .rar, or .7z.".to_owned()),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExternalArchiveFormat {
    Rar,
    SevenZip,
}

fn zip_create_archive(
    reporter: TaskReporter,
    sources: Vec<PathBuf>,
    archive_path: PathBuf,
) -> TaskOutcome {
    let plan = match ArchiveCreatePlan::new(sources, archive_path) {
        Ok(plan) => plan,
        Err(error) => return TaskOutcome::Failed(error),
    };

    reporter.progress(ProgressSnapshot {
        current_file: Some(format!("Creating {}", plan.archive_path.display())),
        total_bytes: Some(plan.totals.bytes),
        total_items: Some(plan.totals.items),
        ..ProgressSnapshot::default()
    });

    let file = match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&plan.archive_path)
    {
        Ok(file) => file,
        Err(error) => {
            return TaskOutcome::Failed(format!(
                "Cannot create {}: {error}",
                plan.archive_path.display()
            ));
        }
    };

    let mut writer = ZipWriter::new(file);
    let mut tracker = ArchiveProgressTracker::new(plan.totals);

    for item in &plan.items {
        if reporter.is_cancelled() {
            let _ = fs::remove_file(&plan.archive_path);
            return TaskOutcome::Cancelled;
        }

        let result = if item.is_directory {
            writer
                .add_directory(&item.archive_name, zip_file_options())
                .map_err(|error| {
                    format!("Cannot add {} to archive: {error}", item.source.display())
                })
        } else {
            zip_add_file(&reporter, &mut tracker, &mut writer, item)
        };

        if let Err(error) = result {
            let _ = fs::remove_file(&plan.archive_path);
            return TaskOutcome::Failed(error);
        }

        tracker.finish_item();
        reporter.progress(tracker.snapshot(&item.source));
    }

    if let Err(error) = writer.finish() {
        let _ = fs::remove_file(&plan.archive_path);
        return TaskOutcome::Failed(format!(
            "Cannot finish {}: {error}",
            plan.archive_path.display()
        ));
    }

    TaskOutcome::Succeeded(TaskCompletionData {
        item_count: plan.items.len(),
        bytes_done: tracker.bytes_done,
    })
}

fn zip_file_options() -> SimpleFileOptions {
    SimpleFileOptions::default().compression_method(CompressionMethod::Deflated)
}

fn zip_add_file<W: Write + io::Seek>(
    reporter: &TaskReporter,
    tracker: &mut ArchiveProgressTracker,
    writer: &mut ZipWriter<W>,
    item: &ArchiveItem,
) -> Result<(), String> {
    writer
        .start_file(&item.archive_name, zip_file_options())
        .map_err(|error| format!("Cannot add {} to archive: {error}", item.source.display()))?;

    let mut input = fs::File::open(&item.source)
        .map_err(|error| format!("Cannot open {}: {error}", item.source.display()))?;
    let mut buffer = [0u8; 64 * 1024];

    loop {
        if reporter.is_cancelled() {
            return Err("Task cancelled.".to_owned());
        }

        let bytes_read = input
            .read(&mut buffer)
            .map_err(|error| format!("Cannot read {}: {error}", item.source.display()))?;
        if bytes_read == 0 {
            break;
        }

        writer.write_all(&buffer[..bytes_read]).map_err(|error| {
            format!(
                "Cannot write {} into archive: {error}",
                item.source.display()
            )
        })?;
        tracker.add_bytes(bytes_read as u64);
        reporter.progress(tracker.snapshot(&item.source));
    }

    Ok(())
}

struct UnarcExtractEntry {
    output_path: PathBuf,
    is_directory: bool,
}

struct UnarcExtractPlan {
    entries: Vec<UnarcExtractEntry>,
    total_bytes: u64,
}

struct ProgressWriter<'a, W: Write> {
    inner: W,
    reporter: &'a TaskReporter,
    tracker: &'a mut ArchiveProgressTracker,
    output_path: &'a Path,
}

impl<W: Write> Write for ProgressWriter<'_, W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.reporter.is_cancelled() {
            return Err(io::Error::other("Task cancelled."));
        }
        let n = self.inner.write(buf)?;
        self.tracker.add_bytes(n as u64);
        self.reporter
            .progress(self.tracker.snapshot(self.output_path));
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

fn unarc_extract_plan(
    archive_path: &Path,
    destination: &Path,
    password: Option<&str>,
) -> Result<UnarcExtractPlan, String> {
    let open_options = unarc_options(password);
    let mut archive = UnarcFormat::open_path_with_options(archive_path, open_options)
        .map_err(|error| format!("Cannot open {}: {error}", archive_path.display()))?;

    let mut entries = Vec::new();
    let mut total_bytes = 0u64;

    loop {
        let entry = match archive.next_entry() {
            Ok(Some(e)) => e,
            Ok(None) => break,
            Err(error) => {
                return Err(unarc_error(
                    &error.to_string(),
                    &format!("Cannot read {}.", archive_path.display()),
                ));
            }
        };

        let name = entry.name();
        let is_directory = name.ends_with('/') || name.ends_with('\\');
        let original_size = entry.original_size();
        let output_path = safe_archive_output_path(destination, name)?;

        total_bytes = total_bytes.saturating_add(original_size);
        entries.push(UnarcExtractEntry {
            output_path,
            is_directory,
        });
    }

    if entries.is_empty() {
        return Err("Archive contains no extractable entries.".to_owned());
    }

    // Some archive formats (e.g. 7z) store directory entries without trailing slashes,
    // so the name-based is_directory check misses them. Re-classify any entry whose
    // output path is an ancestor of another entry's path as a directory.
    let ancestor_paths: std::collections::HashSet<PathBuf> = entries
        .iter()
        .filter(|e| !e.is_directory)
        .flat_map(|e| {
            let mut ancestors = Vec::new();
            let mut path = e.output_path.parent();
            while let Some(parent) = path {
                if parent == destination {
                    break;
                }
                ancestors.push(parent.to_path_buf());
                path = parent.parent();
            }
            ancestors
        })
        .collect();
    for entry in &mut entries {
        if !entry.is_directory && ancestor_paths.contains(&entry.output_path) {
            entry.is_directory = true;
        }
    }

    Ok(UnarcExtractPlan {
        entries,
        total_bytes,
    })
}

fn unarc_extract_archive(
    reporter: TaskReporter,
    archive_path: PathBuf,
    destination: PathBuf,
    password: Option<String>,
) -> TaskOutcome {
    let plan = match unarc_extract_plan(&archive_path, &destination, password.as_deref()) {
        Ok(plan) => plan,
        Err(error) => return TaskOutcome::Failed(error),
    };

    let destination_cleanup = match prepare_extract_destination(&destination) {
        Ok(cleanup) => cleanup,
        Err(error) => return TaskOutcome::Failed(error),
    };

    reporter.progress(ProgressSnapshot {
        current_file: Some(format!("Extracting {}", archive_path.display())),
        total_bytes: Some(plan.total_bytes),
        total_items: Some(plan.entries.len() as u64),
        ..ProgressSnapshot::default()
    });

    let mut archive = match UnarcFormat::open_path_with_options(
        &archive_path,
        unarc_options(password.as_deref()),
    ) {
        Ok(archive) => archive,
        Err(error) => {
            destination_cleanup.clean_up(&destination);
            return TaskOutcome::Failed(format!("Cannot open {}: {error}", archive_path.display()));
        }
    };

    let mut tracker = ArchiveProgressTracker::new(LocalArchiveTotals {
        bytes: plan.total_bytes,
        items: plan.entries.len() as u64,
    });

    let mut plan_idx = 0;
    loop {
        let entry = match archive.next_entry() {
            Ok(Some(e)) => e,
            Ok(None) => break,
            Err(error) => {
                destination_cleanup.clean_up(&destination);
                return TaskOutcome::Failed(unarc_error(
                    &error.to_string(),
                    "Cannot read archive entry.",
                ));
            }
        };

        if reporter.is_cancelled() {
            destination_cleanup.clean_up(&destination);
            return TaskOutcome::Cancelled;
        }

        let Some(plan_entry) = plan.entries.get(plan_idx) else {
            break;
        };
        plan_idx += 1;

        if plan_entry.is_directory {
            if let Err(error) = fs::create_dir_all(&plan_entry.output_path) {
                destination_cleanup.clean_up(&destination);
                return TaskOutcome::Failed(format!(
                    "Cannot create {}: {error}",
                    plan_entry.output_path.display()
                ));
            }
            tracker.finish_item();
            reporter.progress(tracker.snapshot(&plan_entry.output_path));
            continue;
        }

        if let Some(parent) = plan_entry.output_path.parent() {
            if let Err(error) = fs::create_dir_all(parent) {
                destination_cleanup.clean_up(&destination);
                return TaskOutcome::Failed(format!("Cannot create {}: {error}", parent.display()));
            }
        }

        // Guard against archive entries that are directories without trailing slashes
        // (common in 7z): create_dir_all may have already created this path as a directory.
        if plan_entry.output_path.is_dir() {
            tracker.finish_item();
            reporter.progress(tracker.snapshot(&plan_entry.output_path));
            continue;
        }

        let output_file = match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&plan_entry.output_path)
        {
            Ok(file) => file,
            Err(error) => {
                destination_cleanup.clean_up(&destination);
                return TaskOutcome::Failed(format!(
                    "Cannot create {}: {error}",
                    plan_entry.output_path.display()
                ));
            }
        };

        let result = {
            let mut writer = ProgressWriter {
                inner: output_file,
                reporter: &reporter,
                tracker: &mut tracker,
                output_path: &plan_entry.output_path,
            };
            archive.read_to(&entry, &mut writer)
        };

        match result {
            Ok(_) => {}
            Err(error) => {
                destination_cleanup.clean_up(&destination);
                let msg = error.to_string();
                if msg.contains("Task cancelled.") {
                    return TaskOutcome::Cancelled;
                }
                return TaskOutcome::Failed(unarc_error(
                    &msg,
                    &format!("Cannot extract {}.", plan_entry.output_path.display()),
                ));
            }
        }

        tracker.finish_item();
        reporter.progress(tracker.snapshot(&plan_entry.output_path));
    }

    TaskOutcome::Succeeded(TaskCompletionData {
        item_count: tracker.items_done as usize,
        bytes_done: tracker.bytes_done,
    })
}

pub fn check_archive_is_encrypted(path: &Path) -> bool {
    let Ok(_format) = ArchiveFormat::from_path(path) else {
        return false;
    };
    let mut archive = match UnarcFormat::open_path(path) {
        Ok(archive) => archive,
        Err(error) => {
            let msg = error.to_string().to_ascii_lowercase();
            return msg.contains("password") || msg.contains("encrypted");
        }
    };
    loop {
        match archive.next_entry() {
            Ok(Some(entry)) => {
                if entry.is_encrypted() {
                    return true;
                }
            }
            Ok(None) | Err(_) => return false,
        }
    }
}

fn unarc_options(password: Option<&str>) -> ArchiveOptions {
    match password {
        Some(pw) => ArchiveOptions::new().with_password(pw),
        None => ArchiveOptions::new(),
    }
}

fn unarc_error(output: &str, fallback: &str) -> String {
    let lower = output.to_ascii_lowercase();
    if lower.contains("password")
        || lower.contains("encrypted")
        || lower.contains("mac")
        || lower.contains("integrity")
    {
        "Incorrect password or the archive is corrupted.".to_owned()
    } else if output.trim().is_empty() {
        fallback.to_owned()
    } else {
        format!("{fallback} {}", output.trim())
    }
}

fn external_create_archive(
    reporter: TaskReporter,
    format: ExternalArchiveFormat,
    sources: Vec<PathBuf>,
    archive_path: PathBuf,
) -> TaskOutcome {
    let plan = match ArchiveCreatePlan::new_roots_only(sources, archive_path) {
        Ok(plan) => plan,
        Err(error) => return TaskOutcome::Failed(error),
    };

    let tool = match format {
        ExternalArchiveFormat::SevenZip => match find_command(&["7z", "7za", "7zz"]) {
            Some(tool) => tool,
            None => {
                return TaskOutcome::Failed(
                    "7z archive creation requires 7-Zip (7z.exe, 7za.exe, or 7zz.exe) on PATH."
                        .to_owned(),
                );
            }
        },
        ExternalArchiveFormat::Rar => match find_command(&["rar"]) {
            Some(tool) => tool,
            None => {
                return TaskOutcome::Failed(
                    "RAR archive creation requires the WinRAR command line tool rar.exe on PATH."
                        .to_owned(),
                );
            }
        },
    };

    reporter.progress(ProgressSnapshot {
        current_file: Some(format!("Creating {}", plan.archive_path.display())),
        total_bytes: Some(plan.totals.bytes),
        total_items: Some(plan.roots.len() as u64),
        ..ProgressSnapshot::default()
    });

    let mut bytes_done = 0u64;
    let mut items_done = 0u64;
    for root in &plan.roots {
        if reporter.is_cancelled() {
            let _ = fs::remove_file(&plan.archive_path);
            return TaskOutcome::Cancelled;
        }

        let Some(parent) = root.source.parent() else {
            return TaskOutcome::Failed(format!(
                "Cannot determine parent for {}.",
                root.source.display()
            ));
        };
        let Some(file_name) = root.source.file_name() else {
            return TaskOutcome::Failed(format!(
                "Cannot determine name for {}.",
                root.source.display()
            ));
        };

        let mut args = Vec::<OsString>::new();
        match format {
            ExternalArchiveFormat::SevenZip => {
                args.extend(["a", "-t7z", "-y"].into_iter().map(OsString::from));
                args.push(plan.archive_path.as_os_str().to_owned());
                args.push(file_name.to_owned());
            }
            ExternalArchiveFormat::Rar => {
                args.extend(["a", "-r", "-idq"].into_iter().map(OsString::from));
                args.push(plan.archive_path.as_os_str().to_owned());
                args.push(file_name.to_owned());
            }
        }

        let result = run_command_waiting(&reporter, &tool, &args, Some(parent));
        if let Err(error) = result {
            let _ = fs::remove_file(&plan.archive_path);
            if error == "Task cancelled." {
                return TaskOutcome::Cancelled;
            }
            return TaskOutcome::Failed(error);
        }

        bytes_done = bytes_done.saturating_add(root.totals.bytes);
        items_done = items_done.saturating_add(1);
        reporter.progress(ProgressSnapshot {
            current_file: Some(root.source.display().to_string()),
            bytes_done,
            total_bytes: Some(plan.totals.bytes),
            items_done,
            total_items: Some(plan.roots.len() as u64),
        });
    }

    TaskOutcome::Succeeded(TaskCompletionData {
        item_count: plan.roots.len(),
        bytes_done,
    })
}

fn external_extract_archive(
    reporter: TaskReporter,
    tool: PathBuf,
    archive_path: PathBuf,
    destination: PathBuf,
    password: Option<String>,
) -> TaskOutcome {
    let entries = match seven_zip_list_entries(&tool, &archive_path, password.as_deref()) {
        Ok(entries) => entries,
        Err(error) => return TaskOutcome::Failed(error),
    };

    for entry in &entries {
        if let Err(error) = safe_archive_output_path(&destination, entry) {
            return TaskOutcome::Failed(error);
        }
    }

    let destination_cleanup = match prepare_extract_destination(&destination) {
        Ok(cleanup) => cleanup,
        Err(error) => return TaskOutcome::Failed(error),
    };

    reporter.progress(ProgressSnapshot {
        current_file: Some(format!("Extracting {}", archive_path.display())),
        total_items: Some(entries.len() as u64),
        ..ProgressSnapshot::default()
    });

    let mut args = vec![
        OsString::from("x"),
        OsString::from("-y"),
        OsString::from("-bb1"),
        OsString::from(format!("-o{}", destination.display())),
    ];
    if let Some(ref pwd) = password {
        args.push(OsString::from(format!("-p{pwd}")));
    }
    args.push(archive_path.as_os_str().to_owned());

    match run_command_with_output_progress(&reporter, &tool, &args, entries.len() as u64) {
        Ok(items_done) => TaskOutcome::Succeeded(TaskCompletionData {
            item_count: items_done as usize,
            bytes_done: 0,
        }),
        Err(error) if error == "Task cancelled." => {
            destination_cleanup.clean_up(&destination);
            TaskOutcome::Cancelled
        }
        Err(error) => {
            destination_cleanup.clean_up(&destination);
            TaskOutcome::Failed(error)
        }
    }
}

fn seven_zip_list_entries(
    tool: &Path,
    archive_path: &Path,
    password: Option<&str>,
) -> Result<Vec<String>, String> {
    let password_arg = password
        .map(|p| format!("-p{p}"))
        .unwrap_or_else(|| "-p".to_owned());

    let output = Command::new(tool)
        .args(["l", "-slt"])
        .arg(&password_arg)
        .arg(archive_path)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("Cannot run {}: {error}", tool.display()))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}\n{stderr}");
    if !output.status.success() {
        return Err(external_archive_error(
            &combined,
            "Cannot list archive entries.",
        ));
    }

    let mut in_entries = false;
    let mut entries = Vec::new();
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("----------") {
            in_entries = true;
            continue;
        }
        if in_entries
            && let Some(path) = trimmed.strip_prefix("Path = ")
            && !path.trim().is_empty()
        {
            entries.push(path.trim().to_owned());
        }
    }

    if entries.is_empty() {
        return Err("Archive contains no extractable entries.".to_owned());
    }

    Ok(entries)
}

fn run_command_with_output_progress(
    reporter: &TaskReporter,
    tool: &Path,
    args: &[OsString],
    total_items: u64,
) -> Result<u64, String> {
    let mut child = Command::new(tool)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Cannot run {}: {error}", tool.display()))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (line_tx, line_rx) = mpsc::channel::<String>();
    let stdout_handle = stdout.map(|s| spawn_line_reader(s, line_tx.clone()));
    let stderr_handle = stderr.map(|s| spawn_line_reader(s, line_tx));

    let mut seen = HashSet::new();
    let mut items_done = 0u64;
    let mut last_line = String::new();

    loop {
        while let Ok(line) = line_rx.try_recv() {
            let trimmed = line.trim().to_owned();
            if trimmed.is_empty() {
                continue;
            }
            last_line = trimmed.clone();
            if let Some(item) = archive_progress_item(&trimmed)
                && seen.insert(item.clone())
            {
                items_done = items_done.saturating_add(1).min(total_items);
                reporter.progress(ProgressSnapshot {
                    current_file: Some(item),
                    items_done,
                    total_items: Some(total_items),
                    ..ProgressSnapshot::default()
                });
            }
        }

        if reporter.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            join_reader(stdout_handle);
            join_reader(stderr_handle);
            return Err("Task cancelled.".to_owned());
        }

        match child.try_wait() {
            Ok(Some(status)) if status.success() => {
                join_reader(stdout_handle);
                join_reader(stderr_handle);
                reporter.progress(ProgressSnapshot {
                    current_file: Some("Extraction complete".to_owned()),
                    items_done: total_items,
                    total_items: Some(total_items),
                    ..ProgressSnapshot::default()
                });
                return Ok(total_items);
            }
            Ok(Some(status)) => {
                join_reader(stdout_handle);
                join_reader(stderr_handle);
                return Err(external_archive_error(
                    &last_line,
                    &format!("{} failed with status {status}.", tool.display()),
                ));
            }
            Ok(None) => thread::sleep(COMMAND_POLL_INTERVAL),
            Err(error) => {
                join_reader(stdout_handle);
                join_reader(stderr_handle);
                return Err(format!("Cannot wait for {}: {error}", tool.display()));
            }
        }
    }
}

fn spawn_line_reader<R: Read + Send + 'static>(
    reader: R,
    tx: mpsc::Sender<String>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let reader = io::BufReader::new(reader);
        for line in reader.lines().map_while(Result::ok) {
            let _ = tx.send(line);
        }
    })
}

fn join_reader(handle: Option<thread::JoinHandle<()>>) {
    if let Some(handle) = handle {
        let _ = handle.join();
    }
}

fn archive_progress_item(line: &str) -> Option<String> {
    ["Extracting", "Inflating", "Adding", "Compressing"]
        .into_iter()
        .find_map(|prefix| {
            line.strip_prefix(prefix)
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(ToOwned::to_owned)
        })
}

fn external_archive_error(output: &str, fallback: &str) -> String {
    let lower = output.to_ascii_lowercase();
    if lower.contains("password") || lower.contains("encrypted") {
        "This archive is password-protected. Enter the password and try again.".to_owned()
    } else if output.trim().is_empty() {
        fallback.to_owned()
    } else {
        format!("{fallback} {}", output.trim())
    }
}

fn run_command_waiting(
    reporter: &TaskReporter,
    tool: &Path,
    args: &[OsString],
    current_dir: Option<&Path>,
) -> Result<(), String> {
    let mut command = Command::new(tool);
    command.args(args).stdin(Stdio::null());
    command.stdout(Stdio::null()).stderr(Stdio::null());
    if let Some(current_dir) = current_dir {
        command.current_dir(current_dir);
    }

    let mut child = command
        .spawn()
        .map_err(|error| format!("Cannot run {}: {error}", tool.display()))?;

    loop {
        if reporter.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Task cancelled.".to_owned());
        }

        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => {
                return Err(format!("{} failed with status {status}.", tool.display()));
            }
            Ok(None) => thread::sleep(COMMAND_POLL_INTERVAL),
            Err(error) => return Err(format!("Cannot wait for {}: {error}", tool.display())),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ArchiveCreatePlan {
    archive_path: PathBuf,
    roots: Vec<ArchiveRoot>,
    items: Vec<ArchiveItem>,
    totals: LocalArchiveTotals,
}

impl ArchiveCreatePlan {
    fn new(sources: Vec<PathBuf>, archive_path: PathBuf) -> Result<Self, String> {
        let mut plan = Self::new_roots_only(sources, archive_path)?;
        let mut items = Vec::new();
        for root in &plan.roots {
            collect_archive_items(&root.source, &root.archive_name, &mut items)?;
        }
        plan.items = items;
        plan.totals.items = plan.items.len() as u64;
        Ok(plan)
    }

    fn new_roots_only(sources: Vec<PathBuf>, archive_path: PathBuf) -> Result<Self, String> {
        if sources.is_empty() {
            return Err("No items selected for compression.".to_owned());
        }

        let parent = archive_path
            .parent()
            .ok_or_else(|| format!("Cannot determine parent for {}.", archive_path.display()))?;
        if !parent.is_dir() {
            return Err(format!(
                "Target folder {} does not exist.",
                parent.display()
            ));
        }
        if archive_path.exists() {
            return Err(format!("{} already exists.", archive_path.display()));
        }

        let canonical_parent = parent
            .canonicalize()
            .map_err(|error| format!("Cannot inspect {}: {error}", parent.display()))?;
        let archive_file_name = archive_path
            .file_name()
            .ok_or_else(|| format!("Cannot determine name for {}.", archive_path.display()))?;
        let normalized_archive_path = canonical_parent.join(archive_file_name);

        let mut roots = Vec::new();
        let mut totals = LocalArchiveTotals::default();
        for source in sources {
            let file_name = source
                .file_name()
                .ok_or_else(|| format!("Cannot determine name for {}.", source.display()))?;
            let archive_name = path_for_archive(file_name);
            let root_totals = archive_path_totals(&source)
                .map_err(|error| format!("Cannot scan {}: {error}", source.display()))?;

            if source.is_dir() {
                let canonical_source = source
                    .canonicalize()
                    .map_err(|error| format!("Cannot inspect {}: {error}", source.display()))?;
                if normalized_archive_path.starts_with(&canonical_source) {
                    return Err(format!(
                        "Cannot create {} inside selected folder {}.",
                        normalized_archive_path.display(),
                        canonical_source.display()
                    ));
                }
            }

            totals.bytes = totals.bytes.saturating_add(root_totals.bytes);
            totals.items = totals.items.saturating_add(1);
            roots.push(ArchiveRoot {
                source,
                archive_name,
                totals: root_totals,
            });
        }

        Ok(Self {
            archive_path,
            roots,
            items: Vec::new(),
            totals,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ArchiveRoot {
    source: PathBuf,
    archive_name: String,
    totals: LocalArchiveTotals,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ArchiveItem {
    source: PathBuf,
    archive_name: String,
    is_directory: bool,
}

fn collect_archive_items(
    source: &Path,
    archive_name: &str,
    items: &mut Vec<ArchiveItem>,
) -> Result<(), String> {
    let metadata = fs::symlink_metadata(source)
        .map_err(|error| format!("Cannot read {}: {error}", source.display()))?;

    if metadata.is_dir() {
        items.push(ArchiveItem {
            source: source.to_owned(),
            archive_name: ensure_archive_directory_name(archive_name),
            is_directory: true,
        });

        for entry in fs::read_dir(source)
            .map_err(|error| format!("Cannot read {}: {error}", source.display()))?
        {
            let entry =
                entry.map_err(|error| format!("Cannot read {}: {error}", source.display()))?;
            let child_archive_name = format!(
                "{}/{}",
                archive_name.trim_end_matches('/'),
                path_for_archive(&entry.file_name())
            );
            collect_archive_items(&entry.path(), &child_archive_name, items)?;
        }
    } else if metadata.is_file() {
        items.push(ArchiveItem {
            source: source.to_owned(),
            archive_name: archive_name.to_owned(),
            is_directory: false,
        });
    } else {
        return Err(format!("Unsupported file type: {}.", source.display()));
    }

    Ok(())
}

fn path_for_archive(name: &std::ffi::OsStr) -> String {
    name.to_string_lossy().replace('\\', "/")
}

fn ensure_archive_directory_name(name: &str) -> String {
    if name.ends_with('/') {
        name.to_owned()
    } else {
        format!("{name}/")
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct LocalArchiveTotals {
    bytes: u64,
    items: u64,
}

struct ArchiveProgressTracker {
    bytes_done: u64,
    items_done: u64,
    totals: LocalArchiveTotals,
}

impl ArchiveProgressTracker {
    fn new(totals: LocalArchiveTotals) -> Self {
        Self {
            bytes_done: 0,
            items_done: 0,
            totals,
        }
    }

    fn add_bytes(&mut self, bytes: u64) {
        self.bytes_done = self.bytes_done.saturating_add(bytes);
    }

    fn finish_item(&mut self) {
        self.items_done = self.items_done.saturating_add(1);
    }

    fn snapshot(&self, current_file: &Path) -> ProgressSnapshot {
        ProgressSnapshot {
            current_file: Some(current_file.display().to_string()),
            bytes_done: self.bytes_done,
            total_bytes: Some(self.totals.bytes),
            items_done: self.items_done,
            total_items: Some(self.totals.items),
        }
    }
}

fn archive_path_totals(path: &Path) -> Result<LocalArchiveTotals, io::Error> {
    let metadata = fs::symlink_metadata(path)?;
    let mut totals = LocalArchiveTotals { bytes: 0, items: 1 };

    if metadata.is_file() {
        totals.bytes = metadata.len();
    } else if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let child = archive_path_totals(&entry.path())?;
            totals.bytes = totals.bytes.saturating_add(child.bytes);
            totals.items = totals.items.saturating_add(child.items);
        }
    }

    Ok(totals)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExtractDestinationCleanup {
    None,
    RemoveCreatedDirectory,
}

impl ExtractDestinationCleanup {
    fn clean_up(self, destination: &Path) {
        if self == Self::RemoveCreatedDirectory {
            let _ = fs::remove_dir_all(destination);
        }
    }
}

fn prepare_extract_destination(destination: &Path) -> Result<ExtractDestinationCleanup, String> {
    if destination.is_dir() {
        return Ok(ExtractDestinationCleanup::None);
    }

    if destination.exists() {
        return Err(format!(
            "{} exists but is not a folder.",
            destination.display()
        ));
    }

    let parent = destination
        .parent()
        .ok_or_else(|| format!("Cannot determine parent for {}.", destination.display()))?;
    if !parent.is_dir() {
        return Err(format!(
            "Target folder {} does not exist.",
            parent.display()
        ));
    }

    fs::create_dir(destination)
        .map_err(|error| format!("Cannot create {}: {error}", destination.display()))?;

    Ok(ExtractDestinationCleanup::RemoveCreatedDirectory)
}

pub fn safe_archive_output_path(destination: &Path, entry_name: &str) -> Result<PathBuf, String> {
    let normalized = entry_name.replace('\\', "/");
    if normalized.starts_with('/') {
        return Err(format!(
            "Archive entry {entry_name} would escape the destination folder."
        ));
    }

    let trimmed = normalized.trim_matches('/');
    if trimmed.is_empty() {
        return Err("Archive entry has an empty path.".to_owned());
    }

    let mut output = destination.to_path_buf();
    for segment in trimmed.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(format!(
                "Archive entry {entry_name} would escape the destination folder."
            ));
        }
        if segment.contains(':') {
            return Err(format!(
                "Archive entry {entry_name} uses an unsupported Windows path."
            ));
        }

        let path = Path::new(segment);
        if path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        }) {
            return Err(format!(
                "Archive entry {entry_name} would escape the destination folder."
            ));
        }

        output.push(segment);
    }

    Ok(output)
}

fn find_command(names: &[&str]) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    let candidates = env::split_paths(&path)
        .flat_map(|directory| {
            names
                .iter()
                .flat_map(move |name| command_candidates(&directory, name))
        })
        .collect::<Vec<_>>();

    candidates.into_iter().find(|candidate| candidate.is_file())
}

fn command_candidates(directory: &Path, name: &str) -> Vec<PathBuf> {
    let mut candidates = vec![directory.join(name)];

    #[cfg(windows)]
    {
        let has_extension = Path::new(name).extension().is_some();
        if !has_extension {
            let extensions = env::var_os("PATHEXT")
                .map(|value| {
                    value
                        .to_string_lossy()
                        .split(';')
                        .filter(|extension| !extension.is_empty())
                        .map(ToOwned::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_else(|| vec![".EXE".to_owned(), ".BAT".to_owned(), ".CMD".to_owned()]);
            candidates.extend(
                extensions
                    .into_iter()
                    .map(|extension| directory.join(format!("{name}{extension}"))),
            );
        }
    }

    candidates
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_output_accepts_nested_relative_paths() {
        let root = PathBuf::from(r"C:\target");

        let output = safe_archive_output_path(&root, "folder/file.txt").expect("safe path");

        assert_eq!(output, root.join("folder").join("file.txt"));
    }

    #[test]
    fn safe_output_rejects_parent_traversal() {
        let root = PathBuf::from(r"C:\target");

        let error = safe_archive_output_path(&root, "../outside.txt").expect_err("unsafe path");

        assert!(error.contains("escape"));
    }

    #[test]
    fn safe_output_rejects_windows_drive_paths() {
        let root = PathBuf::from(r"C:\target");

        let error =
            safe_archive_output_path(&root, r"C:\Windows\win.ini").expect_err("unsafe path");

        assert!(error.contains("Windows path"));
    }

    #[test]
    fn safe_output_rejects_absolute_paths() {
        let root = PathBuf::from(r"C:\target");

        let error = safe_archive_output_path(&root, "/absolute/file.txt").expect_err("unsafe path");

        assert!(error.contains("empty path") || error.contains("escape"));
    }
}
