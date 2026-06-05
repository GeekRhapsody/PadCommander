use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

use crate::fs;

pub struct TaskManager {
    next_id: u64,
    active: Vec<BackgroundTask>,
    completions_tx: Sender<TaskCompletion>,
    completions_rx: Receiver<TaskCompletion>,
}

impl Default for TaskManager {
    fn default() -> Self {
        let (completions_tx, completions_rx) = mpsc::channel();
        Self {
            next_id: 1,
            active: Vec::new(),
            completions_tx,
            completions_rx,
        }
    }
}

impl TaskManager {
    pub fn summary(&self) -> String {
        if self.active.is_empty() {
            "no active tasks".to_owned()
        } else if self.active.len() == 1 {
            self.active[0].label.clone()
        } else {
            format!("{} active task(s)", self.active.len())
        }
    }

    pub fn spawn_delete_to_recycle_bin(&mut self, label: String, paths: Vec<PathBuf>) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.active.push(BackgroundTask {
            id,
            label: label.clone(),
        });

        let completions_tx = self.completions_tx.clone();
        thread::spawn(move || {
            let result = fs::delete_paths_to_recycle_bin(&paths)
                .map(|()| paths.len())
                .map_err(|error| error.user_message());
            let _ = completions_tx.send(TaskCompletion { id, label, result });
        });

        id
    }

    pub fn poll_completed(&mut self) -> Vec<TaskCompletion> {
        let mut completed = Vec::new();
        while let Ok(completion) = self.completions_rx.try_recv() {
            self.active.retain(|task| task.id != completion.id);
            completed.push(completion);
        }
        completed
    }
}

struct BackgroundTask {
    id: u64,
    label: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskCompletion {
    pub id: u64,
    pub label: String,
    pub result: Result<usize, String>,
}
