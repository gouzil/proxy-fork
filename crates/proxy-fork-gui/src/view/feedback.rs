use super::{dialog, label, primary_button, text};
use crate::app::App;
use imgui::{ImColor32, Key, StyleColor, StyleVar, Ui};

pub(super) fn error_dialog(ui: &Ui, app: &mut App) {
    let Some(error) = &app.error_dialog else {
        return;
    };
    let width = 520.0_f32.min(ui.io().display_size[0] - 32.0);
    let height =
        (ui.calc_text_size_with_opts(error, false, width - 48.0)[1] + 140.0).clamp(216.0, 420.0);
    let Some(_dialog) = dialog(
        ui,
        "feedback.error_title",
        "operation-error",
        [width, height],
    ) else {
        return;
    };
    let _spacing = ui.push_style_var(StyleVar::ItemSpacing([10.0, 10.0]));
    ui.set_window_font_scale(1.2);
    text(ui, "feedback.error_title");
    ui.set_window_font_scale(1.0);
    ui.child_window("error-message")
        .size([0.0, -48.0])
        .build(|| {
            ui.text_wrapped(error);
        });
    ui.set_cursor_pos([ui.window_content_region_max()[0] - 96.0, ui.cursor_pos()[1]]);
    if primary_button(ui, label("feedback.dismiss", "dismiss-error"), [96.0, 34.0])
        || ui.is_key_pressed(Key::Escape)
    {
        app.error_dialog = None;
        ui.close_current_popup();
    }
}

pub(super) fn toast(ui: &Ui, app: &App) {
    let Some(notice) = &app.notice else {
        return;
    };
    let width = 380.0_f32.min(ui.io().display_size[0] - 32.0);
    let message = rust_i18n::t!(notice.as_str());
    let height = ui.calc_text_size_with_opts(&message, false, width - 32.0)[1] + 24.0;
    let origin = [ui.io().display_size[0] - width - 16.0, 16.0];
    let foreground = ui.get_foreground_draw_list();
    foreground
        .add_rect(
            origin,
            [origin[0] + width, origin[1] + height],
            ui.style_color(StyleColor::PopupBg),
        )
        .filled(true)
        .rounding(8.0)
        .build();
    // Keep feedback above modal blur; imgui-rs only exposes wrapped drawing through FFI.
    unsafe {
        imgui::sys::ImDrawList_AddText_FontPtr(
            imgui::sys::igGetForegroundDrawList(),
            std::ptr::null(),
            0.0,
            [origin[0] + 16.0, origin[1] + 12.0].into(),
            ImColor32::from(ui.style_color(StyleColor::Text)).to_bits(),
            message.as_ptr().cast(),
            message.as_ptr().add(message.len()).cast(),
            width - 32.0,
            std::ptr::null(),
        );
    }
}
