use std::{
    collections::{BTreeMap, VecDeque},
    fs as std_fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::Duration,
};

use crate::archive::{ArchiveService, LocalArchiveService};
use crate::fs as commander_fs;

pub type TaskId = u64;

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskKind {
    Copy,
    Move,
    Delete,
    Compress,
    Decompress,
    Upload,
    Download,
    FtpConnect,
}

impl TaskKind {
    pub fn label(self) -> &'static str {
        match self {
            TaskKind::Copy => "Copy",
            TaskKind::Move => "Move",
            TaskKind::Delete => "Delete",
            TaskKind::Compress => "Compress",
            TaskKind::Decompress => "Decompress",
            TaskKind::Upload => "Upload",
            TaskKind::Download => "Download",
            TaskKind::FtpConnect => "FTP",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl TaskStatus {
    pub fn label(self) -> &'static str {
        match self {
            TaskStatus::Queued => "Queued",
            TaskStatus::Running => "Running",
            TaskStatus::Succeeded => "Succeeded",
            TaskStatus::Failed => "Failed",
            TaskStatus::Cancelled => "Cancelled",
        }
    }

    pub fn is_active(self) -> bool {
        matches!(self, TaskStatus::Queued | TaskStatus::Running)
    }

    pub fn is_finished(self) -> bool {
        matches!(
            self,
            TaskStatus::Succeeded | TaskStatus::Failed | TaskStatus::Cancelled
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TaskProgress {
    pub id: TaskId,
    pub kind: TaskKind,
    pub title: String,
    pub current_file: Option<String>,
    pub bytes_done: u64,
    pub total_bytes: Option<u64>,
    pub items_done: u64,
    pub total_items: Option<u64>,
    pub percent: Option<f32>,
    pub status: TaskStatus,
    pub failure: Option<String>,
    pub cancellable: bool,
    pub cancellation_requested: bool,
}

impl TaskProgress {
    pub fn new(id: TaskId, kind: TaskKind, title: String, cancellable: bool) -> Self {
        let mut progress = Self {
            id,
            kind,
            title,
            current_file: None,
            bytes_done: 0,
            total_bytes: None,
            items_done: 0,
            total_items: None,
            percent: None,
            status: TaskStatus::Queued,
            failure: None,
            cancellable,
            cancellation_requested: false,
        };
        progress.recompute_percent();
        progress
    }

    pub fn progress_fraction(&self) -> f32 {
        self.percent.unwrap_or(0.0) / 100.0
    }

    pub fn progress_label(&self) -> String {
        self.percent
            .map(|percent| format!("{percent:.0}%"))
            .unwrap_or_else(|| "Working".to_owned())
    }

    pub fn detail_label(&self) -> String {
        if let Some(failure) = &self.failure {
            return failure.clone();
        }

        if self.cancellation_requested && self.status.is_active() {
            return "Cancellation requested".to_owned();
        }

        self.current_file
            .clone()
            .unwrap_or_else(|| self.status.label().to_owned())
    }

    fn apply_snapshot(&mut self, snapshot: ProgressSnapshot) {
        if self.status == TaskStatus::Queued {
            self.status = TaskStatus::Running;
        }
        self.current_file = snapshot.current_file;
        self.bytes_done = snapshot.bytes_done;
        self.total_bytes = snapshot.total_bytes;
        self.items_done = snapshot.items_done;
        self.total_items = snapshot.total_items;
        self.recompute_percent();
    }

    fn mark_started(&mut self) {
        if self.status == TaskStatus::Queued {
            self.status = TaskStatus::Running;
        }
        self.recompute_percent();
    }

    fn mark_cancel_requested(&mut self) {
        if self.cancellable && self.status.is_active() {
            self.cancellation_requested = true;
        }
    }

    fn mark_finished(&mut self, outcome: &TaskOutcome) {
        self.current_file = None;
        self.cancellation_requested = false;

        match outcome {
            TaskOutcome::Succeeded(data) => {
                self.status = TaskStatus::Succeeded;
                self.failure = None;
                self.bytes_done = self.total_bytes.unwrap_or(data.bytes_done);
                self.total_bytes.get_or_insert(data.bytes_done);
                if let Ok(items_done) = u64::try_from(data.item_count) {
                    self.items_done = self.total_items.unwrap_or(items_done);
                    self.total_items.get_or_insert(items_done);
                }
            }
            TaskOutcome::Failed(error) => {
                self.status = TaskStatus::Failed;
                self.failure = Some(error.clone());
            }
            TaskOutcome::Cancelled => {
                self.status = TaskStatus::Cancelled;
                self.failure = None;
            }
        }

        self.recompute_percent();
    }

    fn recompute_percent(&mut self) {
        self.percent = progress_percent(
            self.bytes_done,
            self.total_bytes,
            self.items_done,
            self.total_items,
            self.status,
        );
    }
}

fn progress_percent(
    bytes_done: u64,
    total_bytes: Option<u64>,
    items_done: u64,
    total_items: Option<u64>,
    status: TaskStatus,
) -> Option<f32> {
    if let Some(total_bytes) = total_bytes
        && total_bytes > 0
    {
        return Some(percent_ratio(bytes_done, total_bytes));
    }

    if let Some(total_items) = total_items
        && total_items > 0
    {
        return Some(percent_ratio(items_done, total_items));
    }

    if status == TaskStatus::Succeeded {
        Some(100.0)
    } else {
        None
    }
}

fn percent_ratio(done: u64, total: u64) -> f32 {
    ((done.min(total) as f64 / total as f64) * 100.0) as f32
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProgressSnapshot {
    pub current_file: Option<String>,
    pub bytes_done: u64,
    pub total_bytes: Option<u64>,
    pub items_done: u64,
    pub total_items: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskCompletionData {
    pub item_count: usize,
    pub bytes_done: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TaskOutcome {
    Succeeded(TaskCompletionData),
    Failed(String),
    Cancelled,
}

impl TaskOutcome {
    fn status(&self) -> TaskStatus {
        match self {
            TaskOutcome::Succeeded(_) => TaskStatus::Succeeded,
            TaskOutcome::Failed(_) => TaskStatus::Failed,
            TaskOutcome::Cancelled => TaskStatus::Cancelled,
        }
    }

    fn result(&self) -> Result<TaskCompletionData, String> {
        match self {
            TaskOutcome::Succeeded(data) => Ok(data.clone()),
            TaskOutcome::Failed(error) => Err(error.clone()),
            TaskOutcome::Cancelled => Err("Task cancelled.".to_owned()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskCompletion {
    pub id: TaskId,
    pub kind: TaskKind,
    pub label: String,
    pub status: TaskStatus,
    pub result: Result<TaskCompletionData, String>,
}

impl TaskCompletion {
    pub fn error_message(&self) -> Option<&str> {
        self.result.as_ref().err().map(String::as_str)
    }
}

pub struct TaskManager {
    next_id: TaskId,
    queue: TaskQueue,
    events_tx: Sender<TaskEvent>,
    events_rx: Receiver<TaskEvent>,
    cancellations: BTreeMap<TaskId, CancellationToken>,
}

impl Default for TaskManager {
    fn default() -> Self {
        let (events_tx, events_rx) = mpsc::channel();
        Self {
            next_id: 1,
            queue: TaskQueue::default(),
            events_tx,
            events_rx,
            cancellations: BTreeMap::new(),
        }
    }
}

impl TaskManager {
    pub fn summary(&self) -> String {
        let active_count = self.queue.active_count();
        let failed_count = self.queue.failed_count();

        match (active_count, failed_count) {
            (0, 0) => "no active tasks".to_owned(),
            (0, 1) => "no active tasks, 1 failure".to_owned(),
            (0, failed_count) => format!("no active tasks, {failed_count} failures"),
            (1, _) => self
                .queue
                .active_task()
                .map(|task| task.title.clone())
                .unwrap_or_else(|| "1 active task".to_owned()),
            (active_count, _) => format!("{active_count} active task(s)"),
        }
    }

    pub fn visible_tasks(&self) -> Vec<&TaskProgress> {
        self.queue.visible_tasks()
    }

    pub fn active_task(&self) -> Option<&TaskProgress> {
        self.queue.active_task()
    }

    pub fn spawn_threaded<F>(
        &mut self,
        kind: TaskKind,
        title: String,
        cancellable: bool,
        work: F,
    ) -> TaskId
    where
        F: FnOnce(TaskReporter) -> TaskOutcome + Send + 'static,
    {
        let id = self.take_next_id();
        let cancellation = CancellationToken::default();

        self.queue
            .enqueue(TaskProgress::new(id, kind, title, cancellable));
        if cancellable {
            self.cancellations.insert(id, cancellation.clone());
        }

        let reporter = TaskReporter {
            id,
            events_tx: self.events_tx.clone(),
            cancellation,
        };

        thread::spawn(move || {
            reporter.started();
            let outcome = work(reporter.clone());
            reporter.finished(outcome);
        });

        id
    }

    pub fn spawn_delete(&mut self, label: String, paths: Vec<PathBuf>, mode: DeleteMode) -> TaskId {
        self.spawn_threaded(TaskKind::Delete, label, true, move |reporter| {
            delete_task(reporter, paths, mode)
        })
    }

    pub fn spawn_copy_to_directory(
        &mut self,
        label: String,
        sources: Vec<PathBuf>,
        target_directory: PathBuf,
    ) -> TaskId {
        self.spawn_threaded(TaskKind::Copy, label, true, move |reporter| {
            local_transfer_task(reporter, LocalTransferKind::Copy, sources, target_directory)
        })
    }

    pub fn spawn_move_to_directory(
        &mut self,
        label: String,
        sources: Vec<PathBuf>,
        target_directory: PathBuf,
    ) -> TaskId {
        self.spawn_threaded(TaskKind::Move, label, true, move |reporter| {
            local_transfer_task(reporter, LocalTransferKind::Move, sources, target_directory)
        })
    }

    pub fn spawn_compress_to_archive(
        &mut self,
        label: String,
        sources: Vec<PathBuf>,
        archive_path: PathBuf,
    ) -> TaskId {
        self.spawn_threaded(TaskKind::Compress, label, true, move |reporter| {
            LocalArchiveService.create_archive(reporter, sources, archive_path)
        })
    }

    pub fn spawn_decompress_archive(
        &mut self,
        label: String,
        archive_path: PathBuf,
        destination: PathBuf,
    ) -> TaskId {
        self.spawn_threaded(TaskKind::Decompress, label, true, move |reporter| {
            LocalArchiveService.extract_archive(reporter, archive_path, destination)
        })
    }

    pub fn spawn_synthetic_task(
        &mut self,
        kind: TaskKind,
        title: String,
        steps: u64,
        fail_at_step: Option<u64>,
    ) -> TaskId {
        let step_count = steps.max(1);
        self.spawn_threaded(kind, title, true, move |reporter| {
            synthetic_task(reporter, step_count, fail_at_step)
        })
    }

    pub fn cancel_active_task(&mut self) -> Option<TaskId> {
        let id = self.queue.active_task().map(|task| task.id)?;
        self.request_cancel(id).then_some(id)
    }

    pub fn request_cancel(&mut self, id: TaskId) -> bool {
        let Some(cancellation) = self.cancellations.get(&id) else {
            return false;
        };

        if !self.queue.is_active(id) {
            return false;
        }

        cancellation.cancel();
        self.queue.mark_cancel_requested(id);
        true
    }

    pub fn poll_completed(&mut self) -> Vec<TaskCompletion> {
        let mut completed = Vec::new();

        while let Ok(event) = self.events_rx.try_recv() {
            if let Some(completion) = self.queue.apply_event(event) {
                self.cancellations.remove(&completion.id);
                completed.push(completion);
            }
        }

        completed
    }

    fn take_next_id(&mut self) -> TaskId {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        id
    }
}

pub struct TaskQueue {
    tasks: BTreeMap<TaskId, TaskProgress>,
    order: VecDeque<TaskId>,
    retained_finished: usize,
}

impl Default for TaskQueue {
    fn default() -> Self {
        Self {
            tasks: BTreeMap::new(),
            order: VecDeque::new(),
            retained_finished: 8,
        }
    }
}

impl TaskQueue {
    pub fn enqueue(&mut self, task: TaskProgress) {
        self.order.push_back(task.id);
        self.tasks.insert(task.id, task);
    }

    pub fn visible_tasks(&self) -> Vec<&TaskProgress> {
        self.order
            .iter()
            .filter_map(|id| self.tasks.get(id))
            .collect()
    }

    pub fn active_task(&self) -> Option<&TaskProgress> {
        self.order
            .iter()
            .filter_map(|id| self.tasks.get(id))
            .find(|task| task.status.is_active())
    }

    pub fn active_count(&self) -> usize {
        self.tasks
            .values()
            .filter(|task| task.status.is_active())
            .count()
    }

    pub fn failed_count(&self) -> usize {
        self.tasks
            .values()
            .filter(|task| task.status == TaskStatus::Failed)
            .count()
    }

    pub fn is_active(&self, id: TaskId) -> bool {
        self.tasks
            .get(&id)
            .map(|task| task.status.is_active())
            .unwrap_or(false)
    }

    pub fn mark_cancel_requested(&mut self, id: TaskId) {
        if let Some(task) = self.tasks.get_mut(&id) {
            task.mark_cancel_requested();
        }
    }

    fn apply_event(&mut self, event: TaskEvent) -> Option<TaskCompletion> {
        match event {
            TaskEvent::Started { id } => {
                if let Some(task) = self.tasks.get_mut(&id) {
                    task.mark_started();
                }
                None
            }
            TaskEvent::Progress { id, snapshot } => {
                if let Some(task) = self.tasks.get_mut(&id)
                    && task.status.is_active()
                {
                    task.apply_snapshot(snapshot);
                }
                None
            }
            TaskEvent::Finished { id, outcome } => {
                let task = self.tasks.get_mut(&id)?;
                task.mark_finished(&outcome);
                let completion = TaskCompletion {
                    id,
                    kind: task.kind,
                    label: task.title.clone(),
                    status: outcome.status(),
                    result: outcome.result(),
                };
                self.trim_finished();
                Some(completion)
            }
        }
    }

    fn trim_finished(&mut self) {
        let mut finished_seen = 0usize;
        let mut remove_ids = Vec::new();

        for id in self.order.iter().rev() {
            let Some(task) = self.tasks.get(id) else {
                remove_ids.push(*id);
                continue;
            };

            if task.status.is_finished() {
                finished_seen += 1;
                if finished_seen > self.retained_finished {
                    remove_ids.push(*id);
                }
            }
        }

        for id in remove_ids {
            self.tasks.remove(&id);
            self.order.retain(|stored_id| *stored_id != id);
        }
    }
}

#[derive(Clone)]
pub struct TaskReporter {
    id: TaskId,
    events_tx: Sender<TaskEvent>,
    cancellation: CancellationToken,
}

impl TaskReporter {
    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    pub fn progress(&self, snapshot: ProgressSnapshot) {
        let _ = self.events_tx.send(TaskEvent::Progress {
            id: self.id,
            snapshot,
        });
    }

    fn started(&self) {
        let _ = self.events_tx.send(TaskEvent::Started { id: self.id });
    }

    fn finished(&self, outcome: TaskOutcome) {
        let _ = self.events_tx.send(TaskEvent::Finished {
            id: self.id,
            outcome,
        });
    }
}

#[derive(Clone, Default)]
struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

#[derive(Clone, Debug)]
enum TaskEvent {
    Started {
        id: TaskId,
    },
    Progress {
        id: TaskId,
        snapshot: ProgressSnapshot,
    },
    Finished {
        id: TaskId,
        outcome: TaskOutcome,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeleteMode {
    RecycleBin,
    Permanent,
}

impl DeleteMode {
    pub fn success_message(self, item_count: usize) -> String {
        match self {
            DeleteMode::RecycleBin => format!("Moved {item_count} item(s) to the Recycle Bin."),
            DeleteMode::Permanent => format!("Permanently deleted {item_count} item(s)."),
        }
    }

    fn ready_label(self) -> &'static str {
        match self {
            DeleteMode::RecycleBin => "Ready to move to Recycle Bin",
            DeleteMode::Permanent => "Ready to permanently delete",
        }
    }
}

fn delete_task(reporter: TaskReporter, paths: Vec<PathBuf>, mode: DeleteMode) -> TaskOutcome {
    if paths.is_empty() {
        return TaskOutcome::Failed("No items selected for deletion.".to_owned());
    }

    let plan = match precompute_local_tree_totals(&paths) {
        Ok(plan) => PlannedDelete {
            roots: plan.roots,
            total_bytes: Some(plan.totals.bytes),
            total_items: Some(plan.totals.items),
        },
        Err(error) => {
            reporter.progress(ProgressSnapshot {
                current_file: Some(format!("Size scan unavailable: {error}")),
                total_items: u64::try_from(paths.len()).ok(),
                ..ProgressSnapshot::default()
            });
            PlannedDelete::fallback(paths)
        }
    };

    let mut bytes_done = 0u64;
    let mut items_done = 0u64;
    let selected_count = plan.roots.len();

    reporter.progress(ProgressSnapshot {
        current_file: Some(mode.ready_label().to_owned()),
        bytes_done,
        total_bytes: plan.total_bytes,
        items_done,
        total_items: plan.total_items,
    });

    for root in &plan.roots {
        if reporter.is_cancelled() {
            return TaskOutcome::Cancelled;
        }

        reporter.progress(ProgressSnapshot {
            current_file: Some(root.path.display().to_string()),
            bytes_done,
            total_bytes: plan.total_bytes,
            items_done,
            total_items: plan.total_items,
        });

        let delete_result = match mode {
            DeleteMode::RecycleBin => {
                commander_fs::delete_paths_to_recycle_bin(std::slice::from_ref(&root.path))
            }
            DeleteMode::Permanent => {
                commander_fs::delete_paths_permanently(std::slice::from_ref(&root.path))
            }
        };

        if let Err(error) = delete_result {
            return TaskOutcome::Failed(error.user_message());
        }

        bytes_done = bytes_done.saturating_add(root.totals.bytes);
        items_done = items_done.saturating_add(root.totals.items);
        reporter.progress(ProgressSnapshot {
            current_file: Some(root.path.display().to_string()),
            bytes_done,
            total_bytes: plan.total_bytes,
            items_done,
            total_items: plan.total_items,
        });
    }

    TaskOutcome::Succeeded(TaskCompletionData {
        item_count: selected_count,
        bytes_done,
    })
}

fn synthetic_task(
    reporter: TaskReporter,
    step_count: u64,
    fail_at_step: Option<u64>,
) -> TaskOutcome {
    let total_bytes = step_count.saturating_mul(64 * 1024);
    reporter.progress(ProgressSnapshot {
        current_file: Some("Preparing".to_owned()),
        total_bytes: Some(total_bytes),
        total_items: Some(step_count),
        ..ProgressSnapshot::default()
    });

    for step in 1..=step_count {
        if reporter.is_cancelled() {
            return TaskOutcome::Cancelled;
        }

        thread::sleep(Duration::from_millis(80));

        if fail_at_step == Some(step) {
            return TaskOutcome::Failed(format!("Synthetic task failed at step {step}."));
        }

        let bytes_done = step.saturating_mul(64 * 1024);
        reporter.progress(ProgressSnapshot {
            current_file: Some(format!("Synthetic item {step}")),
            bytes_done,
            total_bytes: Some(total_bytes),
            items_done: step,
            total_items: Some(step_count),
        });
    }

    let item_count = usize::try_from(step_count).unwrap_or(usize::MAX);
    TaskOutcome::Succeeded(TaskCompletionData {
        item_count,
        bytes_done: total_bytes,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LocalTransferKind {
    Copy,
    Move,
}

impl LocalTransferKind {
    fn verb(self) -> &'static str {
        match self {
            LocalTransferKind::Copy => "Copying",
            LocalTransferKind::Move => "Moving",
        }
    }
}

fn local_transfer_task(
    reporter: TaskReporter,
    kind: LocalTransferKind,
    sources: Vec<PathBuf>,
    target_directory: PathBuf,
) -> TaskOutcome {
    if sources.is_empty() {
        return TaskOutcome::Failed("No items selected.".to_owned());
    }

    let plan = match LocalTransferPlan::new(sources, target_directory) {
        Ok(plan) => plan,
        Err(error) => return TaskOutcome::Failed(error),
    };

    reporter.progress(ProgressSnapshot {
        current_file: Some(format!(
            "{} into {}",
            kind.verb(),
            plan.target_directory.display()
        )),
        total_bytes: Some(plan.totals.bytes),
        total_items: Some(plan.totals.items),
        ..ProgressSnapshot::default()
    });

    let mut tracker = TransferProgressTracker::new(plan.totals);
    for root in &plan.roots {
        if reporter.is_cancelled() {
            return TaskOutcome::Cancelled;
        }

        if let Err(error) = copy_path_recursive(&reporter, &mut tracker, &root.source, &root.target)
        {
            if error == TASK_CANCELLED_MESSAGE {
                return TaskOutcome::Cancelled;
            }
            return TaskOutcome::Failed(error);
        }
    }

    if kind == LocalTransferKind::Move {
        for root in &plan.roots {
            if reporter.is_cancelled() {
                return TaskOutcome::Cancelled;
            }

            if let Err(error) = remove_source_after_move(&root.source) {
                return TaskOutcome::Failed(error);
            }
        }
    }

    TaskOutcome::Succeeded(TaskCompletionData {
        item_count: plan.roots.len(),
        bytes_done: tracker.bytes_done,
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LocalTransferPlan {
    roots: Vec<LocalTransferRoot>,
    target_directory: PathBuf,
    totals: LocalTreeTotals,
}

impl LocalTransferPlan {
    fn new(sources: Vec<PathBuf>, target_directory: PathBuf) -> Result<Self, String> {
        if !target_directory.is_dir() {
            return Err(format!(
                "Target {} is not a directory.",
                target_directory.display()
            ));
        }

        let mut roots = Vec::new();
        let mut totals = LocalTreeTotals::default();

        for source in sources {
            let file_name = source
                .file_name()
                .ok_or_else(|| format!("Cannot determine a name for {}.", source.display()))?;
            let target = target_directory.join(file_name);

            if target.exists() {
                return Err(format!("{} already exists.", target.display()));
            }

            if source.is_dir() && target_directory.starts_with(&source) {
                return Err(format!(
                    "Cannot copy or move {} into itself.",
                    source.display()
                ));
            }

            let root_totals = local_path_totals(&source).map_err(|source_error| {
                format!("Cannot scan {}: {source_error}", source.display())
            })?;
            totals.add(root_totals);
            roots.push(LocalTransferRoot {
                source,
                target,
                totals: root_totals,
            });
        }

        Ok(Self {
            roots,
            target_directory,
            totals,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LocalTransferRoot {
    source: PathBuf,
    target: PathBuf,
    totals: LocalTreeTotals,
}

struct TransferProgressTracker {
    bytes_done: u64,
    items_done: u64,
    totals: LocalTreeTotals,
}

impl TransferProgressTracker {
    fn new(totals: LocalTreeTotals) -> Self {
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

fn copy_path_recursive(
    reporter: &TaskReporter,
    tracker: &mut TransferProgressTracker,
    source: &Path,
    target: &Path,
) -> Result<(), String> {
    if reporter.is_cancelled() {
        return Err(TASK_CANCELLED_MESSAGE.to_owned());
    }

    let metadata = std_fs::symlink_metadata(source)
        .map_err(|error| format!("Cannot read {}: {error}", source.display()))?;

    if metadata.is_dir() {
        std_fs::create_dir(target)
            .map_err(|error| format!("Cannot create {}: {error}", target.display()))?;
        tracker.finish_item();
        reporter.progress(tracker.snapshot(source));

        for entry in std_fs::read_dir(source)
            .map_err(|error| format!("Cannot read {}: {error}", source.display()))?
        {
            let entry =
                entry.map_err(|error| format!("Cannot read {}: {error}", source.display()))?;
            copy_path_recursive(
                reporter,
                tracker,
                &entry.path(),
                &target.join(entry.file_name()),
            )?;
        }
    } else if metadata.is_file() {
        copy_file_with_progress(reporter, tracker, source, target)?;
        tracker.finish_item();
        reporter.progress(tracker.snapshot(source));
    } else {
        return Err(format!("Unsupported file type: {}.", source.display()));
    }

    Ok(())
}

fn copy_file_with_progress(
    reporter: &TaskReporter,
    tracker: &mut TransferProgressTracker,
    source: &Path,
    target: &Path,
) -> Result<(), String> {
    let mut input = std_fs::File::open(source)
        .map_err(|error| format!("Cannot open {}: {error}", source.display()))?;
    let mut output = std_fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)
        .map_err(|error| format!("Cannot create {}: {error}", target.display()))?;
    let mut buffer = [0u8; 64 * 1024];

    loop {
        if reporter.is_cancelled() {
            return Err(TASK_CANCELLED_MESSAGE.to_owned());
        }

        let bytes_read = input
            .read(&mut buffer)
            .map_err(|error| format!("Cannot read {}: {error}", source.display()))?;
        if bytes_read == 0 {
            break;
        }

        output
            .write_all(&buffer[..bytes_read])
            .map_err(|error| format!("Cannot write {}: {error}", target.display()))?;
        tracker.add_bytes(bytes_read as u64);
        reporter.progress(tracker.snapshot(source));
    }

    Ok(())
}

const TASK_CANCELLED_MESSAGE: &str = "Task cancelled.";

fn remove_source_after_move(source: &Path) -> Result<(), String> {
    let metadata = std_fs::symlink_metadata(source)
        .map_err(|error| format!("Cannot read {}: {error}", source.display()))?;

    if metadata.is_dir() {
        std_fs::remove_dir_all(source)
            .map_err(|error| format!("Cannot remove {}: {error}", source.display()))
    } else {
        std_fs::remove_file(source)
            .map_err(|error| format!("Cannot remove {}: {error}", source.display()))
    }
}

struct PlannedDelete {
    roots: Vec<LocalTreeRoot>,
    total_bytes: Option<u64>,
    total_items: Option<u64>,
}

impl PlannedDelete {
    fn fallback(paths: Vec<PathBuf>) -> Self {
        let roots = paths
            .into_iter()
            .map(|path| LocalTreeRoot {
                path,
                totals: LocalTreeTotals { bytes: 0, items: 1 },
            })
            .collect::<Vec<_>>();
        let total_items = u64::try_from(roots.len()).ok();

        Self {
            roots,
            total_bytes: None,
            total_items,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalTreePlan {
    pub roots: Vec<LocalTreeRoot>,
    pub totals: LocalTreeTotals,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalTreeRoot {
    pub path: PathBuf,
    pub totals: LocalTreeTotals,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LocalTreeTotals {
    pub bytes: u64,
    pub items: u64,
}

impl LocalTreeTotals {
    fn add(&mut self, other: Self) {
        self.bytes = self.bytes.saturating_add(other.bytes);
        self.items = self.items.saturating_add(other.items);
    }
}

pub fn precompute_local_tree_totals(paths: &[PathBuf]) -> Result<LocalTreePlan, String> {
    let mut roots = Vec::new();
    let mut totals = LocalTreeTotals::default();

    for path in paths {
        let root_totals = local_path_totals(path)
            .map_err(|source| format!("Cannot scan {}: {source}", path.display()))?;
        totals.add(root_totals);
        roots.push(LocalTreeRoot {
            path: path.clone(),
            totals: root_totals,
        });
    }

    Ok(LocalTreePlan { roots, totals })
}

fn local_path_totals(path: &Path) -> Result<LocalTreeTotals, io::Error> {
    let metadata = std_fs::symlink_metadata(path)?;
    let mut totals = LocalTreeTotals { bytes: 0, items: 1 };

    if metadata.is_file() {
        totals.bytes = metadata.len();
    } else if metadata.is_dir() {
        for entry in std_fs::read_dir(path)? {
            let entry = entry?;
            totals.add(local_path_totals(&entry.path())?);
        }
    }

    Ok(totals)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        io::Write,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn queued_task(id: TaskId) -> TaskProgress {
        TaskProgress::new(id, TaskKind::Copy, format!("Task {id}"), true)
    }

    #[test]
    fn progress_percent_prefers_byte_totals_and_clamps() {
        let mut progress = queued_task(1);
        progress.apply_snapshot(ProgressSnapshot {
            bytes_done: 150,
            total_bytes: Some(100),
            items_done: 1,
            total_items: Some(10),
            current_file: None,
        });

        assert_eq!(progress.percent, Some(100.0));
    }

    #[test]
    fn progress_percent_falls_back_to_item_totals() {
        let mut progress = queued_task(1);
        progress.apply_snapshot(ProgressSnapshot {
            bytes_done: 0,
            total_bytes: None,
            items_done: 2,
            total_items: Some(4),
            current_file: None,
        });

        assert_eq!(progress.percent, Some(50.0));
    }

    #[test]
    fn succeeded_progress_normalizes_to_complete() {
        let mut progress = queued_task(1);
        progress.apply_snapshot(ProgressSnapshot {
            bytes_done: 25,
            total_bytes: Some(100),
            items_done: 1,
            total_items: Some(4),
            current_file: None,
        });

        progress.mark_finished(&TaskOutcome::Succeeded(TaskCompletionData {
            item_count: 2,
            bytes_done: 50,
        }));

        assert_eq!(progress.bytes_done, 100);
        assert_eq!(progress.items_done, 4);
        assert_eq!(progress.percent, Some(100.0));
    }

    #[test]
    fn queue_transitions_from_queued_to_success() {
        let mut queue = TaskQueue::default();
        queue.enqueue(queued_task(7));

        assert_eq!(queue.active_count(), 1);
        assert_eq!(
            queue.tasks.get(&7).map(|task| task.status),
            Some(TaskStatus::Queued)
        );

        queue.apply_event(TaskEvent::Started { id: 7 });
        assert_eq!(
            queue.tasks.get(&7).map(|task| task.status),
            Some(TaskStatus::Running)
        );

        let completion = queue
            .apply_event(TaskEvent::Finished {
                id: 7,
                outcome: TaskOutcome::Succeeded(TaskCompletionData {
                    item_count: 3,
                    bytes_done: 4096,
                }),
            })
            .expect("completion should be emitted");

        assert_eq!(completion.id, 7);
        assert_eq!(completion.status, TaskStatus::Succeeded);
        assert_eq!(queue.active_count(), 0);
        assert_eq!(
            queue.tasks.get(&7).map(|task| task.status),
            Some(TaskStatus::Succeeded)
        );
        assert_eq!(
            queue.tasks.get(&7).and_then(|task| task.percent),
            Some(100.0)
        );
    }

    #[test]
    fn queue_records_failure_without_dropping_other_tasks() {
        let mut queue = TaskQueue::default();
        queue.enqueue(queued_task(1));
        queue.enqueue(queued_task(2));

        let completion = queue
            .apply_event(TaskEvent::Finished {
                id: 1,
                outcome: TaskOutcome::Failed("network unavailable".to_owned()),
            })
            .expect("failure completion should be emitted");

        assert_eq!(completion.status, TaskStatus::Failed);
        assert_eq!(completion.error_message(), Some("network unavailable"));
        assert_eq!(queue.failed_count(), 1);
        assert!(queue.is_active(2));
    }

    #[test]
    fn manager_assigns_stable_increasing_ids() {
        let mut manager = TaskManager::default();
        let first = manager.spawn_threaded(TaskKind::FtpConnect, "First".to_owned(), false, |_| {
            TaskOutcome::Succeeded(TaskCompletionData {
                item_count: 0,
                bytes_done: 0,
            })
        });
        let second =
            manager.spawn_threaded(TaskKind::FtpConnect, "Second".to_owned(), false, |_| {
                TaskOutcome::Succeeded(TaskCompletionData {
                    item_count: 0,
                    bytes_done: 0,
                })
            });

        assert_eq!((first, second), (1, 2));
    }

    #[test]
    fn copy_task_copies_nested_local_tree() {
        let root = unique_test_dir();
        let source = root.join("source");
        let target = root.join("target");
        fs::create_dir_all(source.join("nested")).expect("create source tree");
        fs::create_dir_all(&target).expect("create target dir");
        write_bytes(&source.join("a.bin"), 3);
        write_bytes(&source.join("nested").join("b.bin"), 5);

        let mut manager = TaskManager::default();
        let task_id =
            manager.spawn_copy_to_directory("Copy test".to_owned(), vec![source.clone()], target);
        let completion = wait_for_completion(&mut manager, task_id);

        assert_eq!(completion.status, TaskStatus::Succeeded);
        assert!(source.exists());
        assert_eq!(
            fs::read(root.join("target").join("source").join("a.bin")).expect("read a"),
            vec![b'x'; 3]
        );
        assert_eq!(
            fs::read(
                root.join("target")
                    .join("source")
                    .join("nested")
                    .join("b.bin")
            )
            .expect("read b"),
            vec![b'x'; 5]
        );

        fs::remove_dir_all(&root).expect("remove test dir");
    }

    #[test]
    fn move_task_copies_then_removes_source_tree() {
        let root = unique_test_dir();
        let source = root.join("move-source");
        let target = root.join("move-target");
        fs::create_dir_all(&source).expect("create source dir");
        fs::create_dir_all(&target).expect("create target dir");
        write_bytes(&source.join("moved.bin"), 7);

        let mut manager = TaskManager::default();
        let task_id =
            manager.spawn_move_to_directory("Move test".to_owned(), vec![source.clone()], target);
        let completion = wait_for_completion(&mut manager, task_id);

        assert_eq!(completion.status, TaskStatus::Succeeded);
        assert!(!source.exists());
        assert_eq!(
            fs::read(
                root.join("move-target")
                    .join("move-source")
                    .join("moved.bin")
            )
            .expect("read moved file"),
            vec![b'x'; 7]
        );

        fs::remove_dir_all(&root).expect("remove test dir");
    }

    #[test]
    fn zip_archive_tasks_create_and_extract_nested_tree() {
        let root = unique_test_dir();
        let source = root.join("zip-source");
        let archive_dir = root.join("archives");
        let extract_parent = root.join("extract-parent");
        fs::create_dir_all(source.join("nested")).expect("create source tree");
        fs::create_dir_all(&archive_dir).expect("create archive dir");
        fs::create_dir_all(&extract_parent).expect("create extract parent");
        write_bytes(&source.join("a.bin"), 3);
        write_bytes(&source.join("nested").join("b.bin"), 5);

        let archive_path = archive_dir.join("sample.zip");
        let mut manager = TaskManager::default();
        let task_id = manager.spawn_compress_to_archive(
            "Zip create test".to_owned(),
            vec![source.clone()],
            archive_path.clone(),
        );
        let completion = wait_for_completion(&mut manager, task_id);

        assert_eq!(completion.status, TaskStatus::Succeeded);
        assert!(archive_path.is_file());

        let destination = extract_parent.join("sample");
        let task_id = manager.spawn_decompress_archive(
            "Zip extract test".to_owned(),
            archive_path,
            destination.clone(),
        );
        let completion = wait_for_completion(&mut manager, task_id);

        assert_eq!(completion.status, TaskStatus::Succeeded);
        assert_eq!(
            fs::read(destination.join("zip-source").join("a.bin")).expect("read extracted a"),
            vec![b'x'; 3]
        );
        assert_eq!(
            fs::read(destination.join("zip-source").join("nested").join("b.bin"))
                .expect("read extracted b"),
            vec![b'x'; 5]
        );

        fs::remove_dir_all(&root).expect("remove test dir");
    }

    #[test]
    fn transfer_plan_rejects_existing_target() {
        let root = unique_test_dir();
        let source = root.join("source.txt");
        let target = root.join("target");
        fs::create_dir_all(&target).expect("create target dir");
        write_bytes(&source, 3);
        write_bytes(&target.join("source.txt"), 2);

        let error = LocalTransferPlan::new(vec![source], target).expect_err("target conflict");

        assert!(error.contains("already exists"));
        fs::remove_dir_all(&root).expect("remove test dir");
    }

    #[test]
    fn local_tree_totals_include_nested_files_and_directories() {
        let root = unique_test_dir();
        fs::create_dir_all(root.join("nested")).expect("create nested dir");
        write_bytes(&root.join("a.bin"), 3);
        write_bytes(&root.join("nested").join("b.bin"), 5);

        let plan =
            precompute_local_tree_totals(std::slice::from_ref(&root)).expect("scan test tree");

        assert_eq!(plan.roots.len(), 1);
        assert_eq!(plan.totals.bytes, 8);
        assert_eq!(plan.totals.items, 4);

        fs::remove_dir_all(&root).expect("remove test dir");
    }

    fn unique_test_dir() -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "handheld_commander_task_test_{timestamp}_{}",
            std::process::id()
        ))
    }

    fn write_bytes(path: &Path, len: usize) {
        let mut file = fs::File::create(path).expect("create test file");
        let bytes = vec![b'x'; len];
        file.write_all(&bytes).expect("write test file");
    }

    fn wait_for_completion(manager: &mut TaskManager, task_id: TaskId) -> TaskCompletion {
        for _ in 0..100 {
            if let Some(completion) = manager
                .poll_completed()
                .into_iter()
                .find(|completion| completion.id == task_id)
            {
                return completion;
            }
            thread::sleep(Duration::from_millis(10));
        }

        panic!("task {task_id} did not complete");
    }
}
