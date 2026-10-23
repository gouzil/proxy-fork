use super::{ACCENT, label, muted, text, tooltip};
use crate::{
    app::{Action, App},
    preferences::ThemePreference,
};
use imgui::{Condition, StyleColor, StyleVar, Ui};

fn row(ui: &Ui, key: &str, width: f32, height: f32, control: impl FnOnce(&Ui)) {
    let origin = ui.cursor_pos();
    ui.set_cursor_pos([origin[0], origin[1] + (48.0 - ui.text_line_height()) * 0.5]);
    text(ui, key);
    ui.set_cursor_pos([
        ui.window_size()[0] - 18.0 - width,
        origin[1] + (48.0 - height) * 0.5,
    ]);
    control(ui);
    ui.set_cursor_pos([origin[0], origin[1] + 48.0]);
}

fn switch(ui: &Ui, id: &str, enabled: bool) -> bool {
    let _rounding = ui.push_style_var(StyleVar::FrameRounding(13.0));
    let color = if enabled {
        ACCENT
    } else {
        [0.72, 0.73, 0.75, 1.0]
    };
    let _button = ui.push_style_color(StyleColor::Button, color);
    let _hover = ui.push_style_color(StyleColor::ButtonHovered, color);
    let _active = ui.push_style_color(StyleColor::ButtonActive, color);
    let origin = ui.cursor_screen_pos();
    let clicked = ui.button_with_size(id, [42.0, 26.0]);
    ui.get_window_draw_list()
        .add_circle(
            [
                origin[0] + if enabled { 29.0 } else { 13.0 },
                origin[1] + 13.0,
            ],
            11.0,
            [1.0, 1.0, 1.0, ui.clone_style().alpha],
        )
        .filled(true)
        .build();
    clicked
}

fn autostart(ui: &Ui, app: &App, actions: &mut Vec<Action>) {
    let _disabled = ui.begin_disabled(app.busy || !cfg!(target_os = "macos"));
    row(ui, "settings.autostart", 42.0, 26.0, |ui| {
        let clicked = switch(ui, "###autostart-toggle", app.autostart == Some(true));
        if ui.is_item_hovered() {
            tooltip(ui, || {
                text(
                    ui,
                    match app.autostart {
                        Some(true) => "settings.enabled",
                        Some(false) => "settings.disabled",
                        None if app.error.is_some() => "settings.autostart_recover",
                        None => "settings.unknown",
                    },
                );
                muted(ui, "settings.autostart_note");
            });
        }
        if cfg!(target_os = "macos") {
            if clicked {
                match app.autostart {
                    Some(true) => actions.push(Action::DisableAutostart),
                    Some(false) => actions.push(Action::EnableAutostart),
                    None if app.error.is_some() => ui.open_popup("autostart-recovery"),
                    None => actions.push(Action::InspectAutostart),
                }
            }
            ui.popup("autostart-recovery", || {
                for (key, action) in [
                    ("action.refresh", Action::InspectAutostart),
                    ("settings.enable", Action::EnableAutostart),
                    ("settings.disable", Action::DisableAutostart),
                ] {
                    if ui.menu_item(label(key, key)) {
                        actions.push(action);
                    }
                }
            });
        }
    });
}

fn general(ui: &Ui, app: &mut App, actions: &mut Vec<Action>) {
    row(ui, "settings.language", 164.0, ui.frame_height(), |ui| {
        let preview = rust_i18n::t!(if app.preferences.locale == "zh-CN" {
            "settings.chinese"
        } else {
            "settings.english"
        });
        ui.set_next_item_width(164.0);
        if let Some(_combo) = ui.begin_combo("###locale", preview) {
            for (locale, key) in [("en", "settings.english"), ("zh-CN", "settings.chinese")] {
                if ui
                    .selectable_config(label(key, locale))
                    .selected(app.preferences.locale == locale)
                    .build()
                {
                    app.set_locale(locale);
                }
            }
        }
    });
    row(ui, "settings.appearance", 180.0, 30.0, |ui| {
        let _rounding = ui.push_style_var(StyleVar::FrameRounding(6.0));
        let _border_size = ui.push_style_var(StyleVar::FrameBorderSize(1.0));
        let _border =
            ui.push_style_color(StyleColor::Border, ui.style_color(StyleColor::NavHighlight));
        for (index, (theme, key)) in [
            (ThemePreference::Light, "settings.theme_light"),
            (ThemePreference::Dark, "settings.theme_dark"),
            (ThemePreference::System, "settings.theme_system"),
        ]
        .into_iter()
        .enumerate()
        {
            if index != 0 {
                ui.same_line_with_spacing(0.0, 4.0);
            }
            let selected = app.preferences.theme == theme;
            let _button = ui.push_style_color(
                StyleColor::Button,
                if selected {
                    ACCENT
                } else {
                    ui.style_color(StyleColor::PopupBg)
                },
            );
            let _text = ui.push_style_color(
                StyleColor::Text,
                if selected {
                    [1.0; 4]
                } else {
                    ui.style_color(StyleColor::CheckMark)
                },
            );
            if ui.button_with_size(label(key, key), [(180.0 - 8.0) / 3.0, 30.0]) {
                app.preferences.theme = theme;
            }
        }
    });
    row(ui, "settings.preferences", 148.0, 32.0, |ui| {
        let _disabled = ui.begin_disabled(app.busy);
        if ui.button_with_size(label("settings.save", "save-preferences"), [148.0, 32.0]) {
            actions.push(Action::SavePreferences);
        }
    });
}

fn updates(ui: &Ui, app: &mut App, actions: &mut Vec<Action>) {
    let _platform = ui.begin_disabled(!cfg!(target_os = "macos"));
    row(ui, "settings.updates", 42.0, 26.0, |ui| {
        if switch(
            ui,
            "###automatic-updates",
            app.preferences.automatic_updates,
        ) {
            app.preferences.automatic_updates = !app.preferences.automatic_updates;
        }
        if ui.is_item_hovered() {
            tooltip(ui, || text(ui, "settings.update_frequency"));
        }
    });
    row(ui, "settings.manual_check", 116.0, 32.0, |ui| {
        let _disabled = ui.begin_disabled(app.busy);
        if ui.button_with_size(label("settings.check_now", "check-updates"), [116.0, 32.0]) {
            app.update_dialog = true;
            actions.push(Action::CheckUpdate);
        }
    });
}

pub fn draw(ui: &Ui, app: &mut App, actions: &mut Vec<Action>) {
    let origin = ui.cursor_pos();
    let available = ui.content_region_avail()[0];
    let wide = available >= 656.0;
    let width = if wide {
        (available - 16.0) * 0.5
    } else {
        available
    };
    for (index, (title, x, y, height)) in [
        ("settings.system_group", 0.0, 0.0, 128.0),
        (
            "settings.general_group",
            if wide { width + 16.0 } else { 0.0 },
            if wide { 0.0 } else { 144.0 },
            224.0,
        ),
        (
            "settings.updates_group",
            0.0,
            if wide { 144.0 } else { 384.0 },
            176.0,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        ui.set_cursor_pos([origin[0] + x, origin[1] + y]);
        let _padding = ui.push_style_var(StyleVar::WindowPadding([18.0, 16.0]));
        let _frame_padding = ui.push_style_var(StyleVar::FramePadding([10.0, 6.0]));
        let _background =
            ui.push_style_color(StyleColor::ChildBg, ui.style_color(StyleColor::PopupBg));
        ui.child_window(format!("settings-card-{index}"))
            .size([width, height])
            .border(true)
            .scrollable(false)
            .build(|| {
                ui.set_window_font_scale(1.1);
                text(ui, title);
                ui.set_window_font_scale(1.0);
                ui.set_cursor_pos([18.0, 54.0]);
                match index {
                    0 => autostart(ui, app, actions),
                    1 => general(ui, app, actions),
                    _ => updates(ui, app, actions),
                }
            });
    }
    if cfg!(target_os = "macos") && ui.is_window_appearing() && !app.busy && actions.is_empty() {
        actions.push(Action::InspectAutostart);
    }
    ui.set_cursor_pos([origin[0], origin[1] + if wide { 336.0 } else { 576.0 }]);
    if !cfg!(target_os = "macos") {
        muted(ui, "settings.unsupported");
    }
    muted(ui, "settings.accessibility");
}

pub fn update_dialog(ui: &Ui, app: &mut App, actions: &mut Vec<Action>) {
    if !app.update_dialog {
        return;
    }
    let mut open = true;
    ui.window(label("update.title", "update-dialog"))
        .opened(&mut open)
        .size([720.0, 400.0], Condition::FirstUseEver)
        .build(|| {
            if let Some(version) = &app.update_version {
                text(ui, "update.available");
                ui.text(version);
                ui.text_wrapped(&app.update_notes);
                let disabled = ui.begin_disabled(app.busy || !cfg!(target_os = "macos"));
                if !app.update_downloaded {
                    if ui.button(label("update.download", "download-update")) {
                        actions.push(Action::DownloadUpdate);
                    }
                } else {
                    text(ui, "update.ready");
                    if ui.button(label("update.install", "install-update")) {
                        actions.push(Action::InstallUpdate);
                    }
                }
                drop(disabled);
                if app.update_progress.0 > 0 {
                    ui.text(format!(
                        "{}: {} / {}",
                        rust_i18n::t!("update.progress"),
                        app.update_progress.0,
                        app.update_progress
                            .1
                            .map(|n| n.to_string())
                            .unwrap_or_else(|| "?".into())
                    ));
                }
            } else if !app.busy {
                text(ui, "update.current");
            }
        });
    app.update_dialog = open;
}
