use eframe::egui;

use crate::{
    action::CommanderAction,
    state::AppState,
    tasks::{TaskProgress, TaskStatus},
};

pub fn show(ui: &mut egui::Ui, state: &AppState, actions: &mut Vec<CommanderAction>) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.strong("Tasks");
            ui.separator();
            ui.label(state.task_summary());

            if state.task_manager.active_task().is_some()
                && ui
                    .add_sized([104.0, 28.0], egui::Button::new("Cancel Task"))
                    .clicked()
            {
                actions.push(CommanderAction::CancelActiveTask);
            }
        });

        ui.add_space(4.0);

        if let Some(task) = state.task_manager.active_task() {
            task_row(ui, task, true);
        } else {
            let failures = state
                .task_manager
                .visible_tasks()
                .into_iter()
                .rev()
                .filter(|task| task.status == TaskStatus::Failed)
                .take(1)
                .collect::<Vec<_>>();

            if let Some(task) = failures.first() {
                task_row(ui, task, true);
            } else {
                ui.label("No active tasks.");
            }
        }

        ui.add_space(4.0);
        recent_tasks(ui, state);
    });
}

fn recent_tasks(ui: &mut egui::Ui, state: &AppState) {
    let active_id = state.task_manager.active_task().map(|task| task.id);
    let recent_tasks = state
        .task_manager
        .visible_tasks()
        .into_iter()
        .rev()
        .filter(|task| Some(task.id) != active_id)
        .take(3)
        .collect::<Vec<_>>();

    if recent_tasks.is_empty() {
        return;
    }

    ui.horizontal_wrapped(|ui| {
        for task in recent_tasks {
            compact_task(ui, task);
        }
    });
}

fn task_row(ui: &mut egui::Ui, task: &TaskProgress, prominent: bool) {
    let progress_width = if prominent { 360.0 } else { 220.0 };

    ui.horizontal(|ui| {
        ui.add_sized(
            [58.0, 24.0],
            egui::Label::new(format!("#{}", task.id)).truncate(),
        );
        ui.add_sized([92.0, 24.0], egui::Label::new(task.kind.label()).truncate());
        ui.add_sized([220.0, 24.0], egui::Label::new(&task.title).truncate());
        ui.add(
            egui::ProgressBar::new(task.progress_fraction())
                .desired_width(progress_width)
                .text(task.progress_label()),
        );
        ui.add_sized(
            [104.0, 24.0],
            egui::Label::new(status_text(task)).truncate(),
        );
        ui.add_sized(
            [ui.available_width().max(160.0), 24.0],
            egui::Label::new(detail_text(task)).truncate(),
        );
    });
}

fn compact_task(ui: &mut egui::Ui, task: &TaskProgress) {
    ui.add_sized(
        [292.0, 24.0],
        egui::Label::new(format!(
            "#{} {} {} {}",
            task.id,
            task.kind.label(),
            task.status.label(),
            task.progress_label()
        ))
        .truncate(),
    );
}

fn status_text(task: &TaskProgress) -> egui::RichText {
    let color = match task.status {
        TaskStatus::Failed => egui::Color32::LIGHT_RED,
        TaskStatus::Cancelled => egui::Color32::from_rgb(220, 184, 110),
        TaskStatus::Succeeded => egui::Color32::from_rgb(132, 184, 153),
        TaskStatus::Queued | TaskStatus::Running => egui::Color32::WHITE,
    };

    egui::RichText::new(task.status.label()).color(color)
}

fn detail_text(task: &TaskProgress) -> egui::RichText {
    let text = task.detail_label();
    if task.status == TaskStatus::Failed {
        egui::RichText::new(text).color(egui::Color32::LIGHT_RED)
    } else {
        egui::RichText::new(text)
    }
}
