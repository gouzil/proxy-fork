use super::{label, text};
use crate::{app::App, logging::LogBuffer};
use imgui::Ui;
use std::sync::atomic::Ordering;

pub fn draw(ui: &Ui, app: &mut App, log: &LogBuffer) {
    if ui.button(label("logs.clear", "clear-log")) {
        log.clear();
    }
    ui.same_line();
    ui.checkbox(
        label("logs.autoscroll", "auto-scroll"),
        &mut app.auto_scroll,
    );
    ui.combo_simple_string(
        label("logs.level", "log-level"),
        &mut app.log_level,
        &["TRACE", "DEBUG", "INFO", "WARN", "ERROR"],
    );
    ui.text(format!(
        "{}: {}",
        rust_i18n::t!("logs.dropped"),
        log.dropped.load(Ordering::Relaxed)
    ));
    text(ui, "logs.privacy");
    ui.child_window("log-lines")
        .size([0.0, 0.0])
        .horizontal_scrollbar(true)
        .build(|| {
            if let Ok(records) = log.records.lock() {
                let levels = [
                    tracing::Level::TRACE,
                    tracing::Level::DEBUG,
                    tracing::Level::INFO,
                    tracing::Level::WARN,
                    tracing::Level::ERROR,
                ];
                let filtered: Vec<_> = records
                    .iter()
                    .filter(|r| r.level <= levels[app.log_level.min(4)])
                    .collect();
                let mut clipper = imgui::ListClipper::new(filtered.len() as i32)
                    .items_height(ui.text_line_height_with_spacing())
                    .begin(ui);
                while clipper.step() {
                    for index in clipper.display_start()..clipper.display_end() {
                        let record = filtered[index as usize];
                        let _id = ui.push_id_int(index);
                        if ui.selectable_config(&record.line).build() {
                            app.log_detail = Some(record.line.clone());
                        }
                    }
                }
                if app.auto_scroll {
                    ui.set_scroll_here_y_with_ratio(1.0);
                }
            }
        });
}
