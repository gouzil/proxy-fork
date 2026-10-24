use crate::{
    app::{Action, App, Page},
    logging::LogBuffer,
};
use imgui::{Condition, StyleColor, StyleVar, Ui};
use proxy_fork_core::proxy_service::ProxyServiceState;
pub mod dashboard;
pub mod logs;
pub mod rules;
pub mod settings;

pub fn label(key: &str, id: &str) -> String {
    format!("{}###{id}", rust_i18n::t!(key))
}
pub fn text(ui: &Ui, key: &str) {
    ui.text_wrapped(rust_i18n::t!(key));
}

pub fn tooltip(ui: &Ui, content: impl FnOnce()) {
    ui.tooltip(|| {
        // Auto-sized tooltips need a wrap width independent of their first layout pass.
        let width = (ui.io().display_size[0] - 48.0).clamp(1.0, 420.0);
        let _wrap = ui.push_text_wrap_pos_with_pos(ui.cursor_pos()[0] + width);
        content();
    });
}

pub const ACCENT: [f32; 4] = [0.16, 0.43, 0.94, 1.0];

pub fn muted(ui: &Ui, key: &str) {
    let _color = ui.push_style_color(StyleColor::Text, ui.style_color(StyleColor::TextDisabled));
    text(ui, key);
}

pub fn primary_button(ui: &Ui, title: String, size: [f32; 2]) -> bool {
    let _background = ui.push_style_color(StyleColor::Button, ACCENT);
    let _hover = ui.push_style_color(StyleColor::ButtonHovered, [0.12, 0.37, 0.86, 1.0]);
    let _active = ui.push_style_color(StyleColor::ButtonActive, [0.09, 0.31, 0.75, 1.0]);
    let _text = ui.push_style_color(StyleColor::Text, [1.0; 4]);
    ui.button_with_size(title, size)
}

pub fn state_key(app: &App) -> &'static str {
    match app.state {
        ProxyServiceState::Stopped => "state.stopped",
        ProxyServiceState::Starting => "state.starting",
        ProxyServiceState::Running(_) => "state.running",
        ProxyServiceState::Stopping => "state.stopping",
        ProxyServiceState::Failed(_) => "state.failed",
    }
}

fn dialog<'ui>(ui: &'ui Ui, key: &str, id: &str, size: [f32; 2]) -> Option<imgui::PopupToken<'ui>> {
    let title = label(key, id);
    let popup_id = std::ffi::CString::new(format!("###{id}")).expect("static dialog ID");
    let viewport = ui.io().display_size;
    // imgui-rs 0.12 does not expose popup queries or next-modal geometry as safe methods.
    unsafe {
        if !imgui::sys::igIsPopupOpen(popup_id.as_ptr(), 0) {
            ui.open_popup(&title);
        }
        imgui::sys::igSetNextWindowPos(
            [viewport[0] * 0.5, viewport[1] * 0.5].into(),
            imgui::sys::ImGuiCond_Always as i32,
            [0.5, 0.5].into(),
        );
        imgui::sys::igSetNextWindowSize(
            [
                size[0].min(viewport[0] - 32.0),
                size[1].min(viewport[1] - 32.0),
            ]
            .into(),
            imgui::sys::ImGuiCond_Always as i32,
        );
    }
    let _padding = ui.push_style_var(StyleVar::WindowPadding([24.0, 20.0]));
    let _rounding = ui.push_style_var(StyleVar::WindowRounding(12.0));
    let token = ui
        .modal_popup_config(title)
        .title_bar(false)
        .resizable(false)
        .movable(false)
        .save_settings(false)
        .begin_popup()?;
    crate::render::backdrop::mark_modal();
    Some(token)
}

fn sidebar(ui: &Ui, app: &mut App) {
    ui.set_cursor_pos([32.0, 24.0]);
    ui.set_window_font_scale(1.35);
    ui.text("Proxy Fork");
    ui.set_window_font_scale(1.0);
    ui.set_cursor_pos([14.0, ui.cursor_pos()[1] + 12.0]);
    let _align = ui.push_style_var(StyleVar::ButtonTextAlign([0.0, 0.5]));
    let _padding = ui.push_style_var(StyleVar::FramePadding([18.0, 8.0]));
    let width = ui.window_size()[0] - 28.0;
    for (page, key) in [
        (Page::Dashboard, "nav.dashboard"),
        (Page::Rules, "nav.rules"),
        (Page::Logs, "nav.logs"),
        (Page::Settings, "nav.settings"),
    ] {
        let selected = app.page == page;
        let _background = ui.push_style_color(
            StyleColor::Button,
            if selected {
                ui.style_color(StyleColor::Header)
            } else {
                [0.0; 4]
            },
        );
        let _text = ui.push_style_color(
            StyleColor::Text,
            if selected {
                ui.style_color(StyleColor::CheckMark)
            } else {
                ui.style_color(StyleColor::Text)
            },
        );
        if ui.button_with_size(label(key, key), [width, 40.0]) {
            app.page = page;
        }
    }
    let _spacing = ui.push_style_var(StyleVar::ItemSpacing([0.0, 6.0]));
    ui.set_cursor_pos([
        14.0,
        ui.window_size()[1] - 3.0 * ui.text_line_height_with_spacing() - 32.0,
    ]);
    ui.separator();
    ui.set_cursor_pos([32.0, ui.cursor_pos()[1] + 6.0]);
    ui.group(|| {
        muted(ui, "sidebar.local");
        text(ui, state_key(app));
        ui.text_colored(
            ui.style_color(StyleColor::TextDisabled),
            format!("v{}", env!("CARGO_PKG_VERSION")),
        );
    });
}

pub fn draw(ui: &Ui, app: &mut App, log: &LogBuffer, theme: winit::window::Theme) -> Vec<Action> {
    let mut actions = Vec::new();
    let _padding = ui.push_style_var(StyleVar::WindowPadding([0.0, 0.0]));
    let _spacing = ui.push_style_var(StyleVar::ItemSpacing([0.0, 0.0]));
    ui.window("Proxy Fork###main")
        .position([0.0, 0.0], Condition::Always)
        .size(ui.io().display_size, Condition::Always)
        .title_bar(false)
        .movable(false)
        .resizable(false)
        .collapsible(false)
        .scroll_bar(false)
        .build(|| {
            {
                let _background =
                    ui.push_style_color(StyleColor::ChildBg, ui.style_color(StyleColor::TitleBg));
                let _spacing = ui.push_style_var(StyleVar::ItemSpacing([0.0, 10.0]));
                let _padding = ui.push_style_var(StyleVar::WindowPadding([14.0, 0.0]));
                let _rounding = ui.push_style_var(StyleVar::ChildRounding(0.0));
                let width = if ui.io().display_size[0] < 800.0 {
                    164.0
                } else {
                    196.0
                };
                let previous_page = app.page;
                ui.child_window("sidebar")
                    .size([width, 0.0])
                    .always_use_window_padding(true)
                    .scrollable(false)
                    .build(|| sidebar(ui, app));
                if previous_page == Page::Dashboard && app.page != previous_page {
                    app.request_config_save();
                }
            }
            ui.same_line_with_spacing(0.0, 0.0);
            let _background =
                ui.push_style_color(StyleColor::ChildBg, ui.style_color(StyleColor::WindowBg));
            let _padding = ui.push_style_var(StyleVar::WindowPadding([24.0, 20.0]));
            let _spacing = ui.push_style_var(StyleVar::ItemSpacing([10.0, 10.0]));
            let _rounding = ui.push_style_var(StyleVar::ChildRounding(0.0));
            ui.child_window("workspace")
                .size([0.0, 0.0])
                .always_use_window_padding(true)
                .scrollable(false)
                .build(|| {
                    let (title, subtitle) = match app.page {
                        Page::Dashboard => ("nav.dashboard", "dashboard.subtitle"),
                        Page::Rules => ("nav.rules", "rules.subtitle"),
                        Page::Logs => ("nav.logs", "logs.subtitle"),
                        Page::Settings => ("nav.settings", "settings.subtitle"),
                    };
                    let top = ui.cursor_pos()[1];
                    ui.set_window_font_scale(1.45);
                    text(ui, title);
                    ui.set_window_font_scale(1.0);
                    let below_title = ui.cursor_pos();
                    if app.page == Page::Logs {
                        logs::toolbar(ui, app, log, top);
                        ui.set_cursor_pos(below_title);
                    }
                    muted(ui, subtitle);
                    ui.dummy([0.0, 6.0]);
                    if app.busy {
                        muted(ui, "state.working");
                    }
                    for error in app
                        .error
                        .iter()
                        .chain(app.rule_save_error.iter())
                        .chain(app.config_save_error.iter())
                    {
                        let _error = ui.push_style_color(
                            StyleColor::Text,
                            if theme == winit::window::Theme::Dark {
                                [1.0, 0.54, 0.56, 1.0]
                            } else {
                                [0.74, 0.19, 0.22, 1.0]
                            },
                        );
                        text(ui, "state.error");
                        ui.text_wrapped(error);
                    }
                    if let Some(notice) = &app.notice {
                        text(ui, notice);
                    }
                    let _background = ui.push_style_color(StyleColor::ChildBg, [0.0; 4]);
                    let _rounding = ui.push_style_var(StyleVar::ChildRounding(10.0));
                    ui.child_window(format!("page-{title}"))
                        .size([0.0, 0.0])
                        .scrollable(app.page != Page::Logs)
                        .build(|| match app.page {
                            Page::Dashboard => dashboard::draw(ui, app, &mut actions, theme),
                            Page::Rules => rules::draw(ui, app),
                            Page::Logs => logs::draw(ui, app, log),
                            Page::Settings => settings::draw(ui, app, &mut actions),
                        });
                });
        });
    drop(_spacing);
    drop(_padding);
    rules::editor(ui, app);
    dashboard::certificate_dialog(ui, app, &mut actions);
    settings::update_dialog(ui, app, &mut actions);
    logs::details(ui, app);
    actions
}
