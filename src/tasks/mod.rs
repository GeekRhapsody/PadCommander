#[derive(Clone, Debug, Default)]
pub struct TaskManager;

impl TaskManager {
    pub fn summary(&self) -> &'static str {
        "no active tasks"
    }
}
