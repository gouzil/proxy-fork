use super::{ACCENT, dialog, label, muted, primary_button, text};
use crate::{app::App, logging::LogBuffer};
use imgui::{Key, StyleColor, StyleVar, Ui};
use std::sync::atomic::Ordering;

fn level_color(ui: &Ui, level: &str) -> [f32; 4] {
    match level {
        "ERROR" => [0.91, 0.30, 0.32, 1.0],
        "WARN" => [0.78, 0.52, 0.12, 1.0],
        "INFO" => ACCENT,
        _ => ui.style_color(StyleColor::TextDisabled),
    }
}

pub fn details(ui: &Ui, app: &mut App) {
    let Some(detail) = app.log_detail.as_deref() else {
        return;
    };
    let (timestamp, rest) = detail.split_once(' ').unwrap_or(("", detail));
    let (level, message) = rest.split_once(' ').unwrap_or(("", rest));
    let message = message.trim_start();
    let width = 600.0_f32.min(ui.io().display_size[0] - 32.0);
    let text_height = ui.calc_text_size_with_opts(message, false, width - 96.0)[1];
    let height = (text_height + 32.0).clamp(84.0, 220.0) + 150.0;
    let _spacing = ui.push_style_var(StyleVar::ItemSpacing([10.0, 10.0]));
    let Some(_dialog) = dialog(ui, "logs.details", "log-details", [width, height]) else {
        return;
    };
    ui.set_window_font_scale(1.2);
    text(ui, "logs.details");
    ui.set_window_font_scale(1.0);
    ui.text_colored(
        ui.style_color(StyleColor::TextDisabled),
        format!("{timestamp} UTC"),
    );
    ui.same_line_with_spacing(0.0, 12.0);
    ui.text_colored(level_color(ui, level), level);
    {
        let _padding = ui.push_style_var(StyleVar::WindowPadding([16.0, 16.0]));
        let _rounding = ui.push_style_var(StyleVar::ChildRounding(8.0));
        let _background =
            ui.push_style_color(StyleColor::ChildBg, ui.style_color(StyleColor::FrameBg));
        ui.child_window("log-detail-message")
            .size([0.0, -48.0])
            .always_use_window_padding(true)
            .border(true)
            .build(|| ui.text_wrapped(message));
    }
    ui.set_cursor_pos([
        ui.window_content_region_max()[0] - 96.0,
        ui.window_size()[1] - 54.0,
    ]);
    if ui.button_with_size(label("action.close", "close-log-details"), [96.0, 34.0])
        || ui.is_key_pressed(Key::Escape)
    {
        app.log_detail = None;
        ui.close_current_popup();
    }
}

pub fn toolbar(ui: &Ui, app: &mut App, log: &LogBuffer, top: f32) {
    let _padding = ui.push_style_var(StyleVar::FramePadding([12.0, 8.0]));
    let pause = if app.paused_logs.is_some() {
        "logs.resume"
    } else {
        "logs.pause"
    };
    let order = if app.log_newest_first {
        "logs.newest"
    } else {
        "logs.oldest"
    };
    let width: f32 = [pause, order, "logs.clear"]
        .iter()
        .map(|key| ui.calc_text_size(rust_i18n::t!(*key))[0] + 24.0)
        .sum::<f32>()
        + 16.0;
    ui.set_cursor_pos([ui.window_content_region_max()[0] - width, top]);
    if ui.button_with_size(label(pause, "pause-log"), [0.0, 34.0]) {
        app.paused_logs = if app.paused_logs.is_some() {
            None
        } else {
            log.records.lock().ok().map(|records| records.clone())
        };
        app.auto_scroll = true;
    }
    ui.same_line_with_spacing(0.0, 8.0);
    if ui.button_with_size(label(order, "log-order"), [0.0, 34.0]) {
        app.log_newest_first = !app.log_newest_first;
        app.auto_scroll = true;
    }
    ui.same_line_with_spacing(0.0, 8.0);
    if primary_button(ui, label("logs.clear", "clear-log"), [0.0, 34.0]) {
        log.clear();
        if let Some(records) = &mut app.paused_logs {
            records.clear();
        }
        app.log_detail = None;
    }
}

pub fn draw(ui: &Ui, app: &mut App, log: &LogBuffer) {
    let _padding = ui.push_style_var(StyleVar::FramePadding([12.0, 8.0]));
    ui.set_next_item_width(112.0);
    if ui.combo_simple_string(
        "###log-level",
        &mut app.log_level,
        &["ALL", "TRACE", "DEBUG", "INFO", "WARN", "ERROR"],
    ) {
        app.auto_scroll = true;
    }
    ui.same_line_with_spacing(0.0, 10.0);
    ui.set_next_item_width(-1.0);
    if ui
        .input_text("###log-filter", &mut app.log_filter)
        .hint(rust_i18n::t!("logs.filter"))
        .build()
    {
        app.auto_scroll = true;
    }
    let dropped = log.dropped.load(Ordering::Relaxed);
    if dropped > 0 {
        ui.text(format!("{}: {dropped}", rust_i18n::t!("logs.dropped")));
    }
    ui.child_window("log-lines")
        .size([0.0, -(ui.text_line_height() * 2.0 + 10.0)])
        .horizontal_scrollbar(true)
        .build(|| {
            let live = if app.paused_logs.is_none() {
                log.records.lock().ok()
            } else {
                None
            };
            let Some(records) = app.paused_logs.as_ref().or(live.as_deref()) else {
                return;
            };
            let filter = app.log_filter.to_lowercase();
            let levels = [
                tracing::Level::TRACE,
                tracing::Level::TRACE,
                tracing::Level::DEBUG,
                tracing::Level::INFO,
                tracing::Level::WARN,
                tracing::Level::ERROR,
            ];
            let mut filtered: Vec<_> = records
                .iter()
                .filter(|record| {
                    record.level <= levels[app.log_level.min(5)]
                        && (filter.is_empty() || record.line.to_lowercase().contains(&filter))
                })
                .collect();
            if app.log_newest_first {
                filtered.reverse();
            }
            if filtered.is_empty() {
                muted(ui, "logs.empty");
                return;
            }
            let follow = std::mem::take(&mut app.auto_scroll)
                || if app.log_newest_first {
                    ui.scroll_y() <= 1.0
                } else {
                    ui.scroll_y() >= ui.scroll_max_y() - 1.0
                };
            let height = ui.text_line_height() * 2.0 + 20.0;
            let mut clipper = imgui::ListClipper::new(filtered.len() as i32)
                .items_height(height)
                .begin(ui);
            while clipper.step() {
                for index in clipper.display_start()..clipper.display_end() {
                    let record = filtered[index as usize];
                    let _id = ui.push_id_int(index);
                    let origin = ui.cursor_pos();
                    if ui
                        .selectable_config("###log-record")
                        .size([0.0, height - 10.0])
                        .build()
                    {
                        app.log_detail = Some(record.line.clone());
                    }
                    ui.set_cursor_pos([origin[0], origin[1] + 4.0]);
                    let (timestamp, rest) =
                        record.line.split_once(' ').unwrap_or(("", &record.line));
                    ui.text_colored(ui.style_color(StyleColor::TextDisabled), timestamp);
                    ui.same_line_with_spacing(0.0, 10.0);
                    ui.text_colored(
                        level_color(ui, record.level.as_str()),
                        record.level.as_str(),
                    );
                    ui.set_cursor_pos([origin[0], origin[1] + ui.text_line_height() + 6.0]);
                    ui.text(
                        rest.strip_prefix(record.level.as_str())
                            .unwrap_or(rest)
                            .trim_start(),
                    );
                    ui.set_cursor_pos([origin[0], origin[1] + height - 5.0]);
                    ui.separator();
                    ui.set_cursor_pos([origin[0], origin[1] + height]);
                }
            }
            if follow {
                if app.log_newest_first {
                    ui.set_scroll_y(0.0);
                } else {
                    ui.set_scroll_here_y_with_ratio(1.0);
                }
            }
        });
    muted(ui, "logs.privacy");
}
