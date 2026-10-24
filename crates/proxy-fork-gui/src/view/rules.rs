use super::{dialog, label, muted, primary_button, text, tooltip};
use crate::app::{App, RuleDraft};
use imgui::{Key, StyleColor, StyleVar, Ui};

fn field(ui: &Ui, key: &str, id: &str, value: &mut String, decimal: bool) -> bool {
    muted(ui, key);
    ui.set_next_item_width(-1.0);
    ui.input_text(format!("###{id}"), value)
        .chars_decimal(decimal)
        .build();
    ui.is_item_deactivated_after_edit()
}

fn optional_field(
    ui: &Ui,
    key: &str,
    id: &str,
    value: &mut Option<String>,
    literal_empty: &mut bool,
) -> bool {
    let mut input = value.clone().unwrap_or_default();
    let edited = field(ui, key, id, &mut input, false);
    if ui.is_item_activated() {
        // Escape must restore the distinction between a loaded empty path and no path.
        *literal_empty = value.as_ref().is_some_and(String::is_empty);
    }
    let cancelled = ui.is_item_deactivated() && ui.is_key_pressed(Key::Escape);
    if input != value.as_deref().unwrap_or_default() || (cancelled && *literal_empty) {
        *value = if input.is_empty() && !(cancelled && *literal_empty) {
            None
        } else {
            Some(input)
        };
    }
    edited
}

fn endpoint(protocol: &str, host: &str, port: &str, path: Option<&str>) -> String {
    let port = if port.is_empty() {
        String::new()
    } else {
        format!(":{port}")
    };
    format!("{protocol}://{host}{port}{}", path.unwrap_or_default())
}

pub fn draw(ui: &Ui, app: &mut App) {
    let _disabled = ui.begin_disabled(app.busy);
    let origin = ui.cursor_pos();
    let width = ui.content_region_avail()[0];
    ui.set_cursor_pos([origin[0] + 16.0, origin[1] + 16.0]);
    if primary_button(ui, label("rules.add", "add-rule"), [112.0, 34.0]) {
        app.add_rule();
        app.rule_editor = app.rules.last().map(|rule| rule.id);
    }
    ui.set_cursor_pos([origin[0], origin[1] + 66.0]);
    if app.rules.is_empty() {
        let height = ui.content_region_avail()[1];
        ui.set_cursor_pos([origin[0], ui.cursor_pos()[1] + (height * 0.32).max(16.0)]);
        for key in ["rules.empty", "rules.empty_hint"] {
            let title = rust_i18n::t!(key);
            ui.set_cursor_pos([
                origin[0] + ((width - ui.calc_text_size(&title)[0]) * 0.5).max(0.0),
                ui.cursor_pos()[1],
            ]);
            if key == "rules.empty" {
                ui.text(title);
            } else {
                muted(ui, key);
            }
        }
        return;
    }
    let count = app.rules.len();
    let mut delete = None;
    let mut swap = None;
    let _padding = ui.push_style_var(StyleVar::WindowPadding([16.0, 14.0]));
    let _background = ui.push_style_color(StyleColor::ChildBg, ui.style_color(StyleColor::PopupBg));
    let _spacing = ui.push_style_var(StyleVar::ItemSpacing([10.0, 8.0]));
    for (i, draft) in app.rules.iter().enumerate() {
        let _id = ui.push_id(draft.id.to_string());
        let source = endpoint(
            &draft.rule.protocol,
            &draft.rule.host,
            &draft.source_port,
            draft.rule.path.as_deref(),
        );
        let target = endpoint(
            draft
                .rule
                .target_protocol
                .as_deref()
                .filter(|s| !s.is_empty())
                .unwrap_or("http"),
            &draft.rule.target_host,
            &draft.target_port,
            draft.rule.target_path.as_deref(),
        );
        ui.child_window("rule-row")
            .size([0.0, 76.0])
            .always_use_window_padding(true)
            .scrollable(false)
            .build(|| {
                let top = ui.cursor_pos()[1];
                ui.text_colored(
                    ui.style_color(StyleColor::TextDisabled),
                    format!("{:02}", i + 1),
                );
                ui.same_line_with_pos(48.0);
                ui.child_window("summary")
                    .size([(ui.content_region_avail()[0] - 156.0).max(20.0), 48.0])
                    .scrollable(false)
                    .build(|| {
                        ui.text(&source);
                        ui.text_colored(
                            ui.style_color(StyleColor::TextDisabled),
                            format!("{}  {}", rust_i18n::t!("rules.forward_to"), target),
                        );
                        if ui.is_window_hovered() {
                            tooltip(ui, || {
                                ui.text_wrapped(&source);
                                ui.text_wrapped(&target);
                            });
                        }
                    });
                ui.set_cursor_pos([ui.window_size()[0] - 156.0, top + 7.0]);
                if ui.button_with_size(label("rules.edit", "edit"), [96.0, 32.0]) {
                    app.rule_editor = Some(draft.id);
                }
                ui.same_line_with_spacing(0.0, 8.0);
                if ui.button_with_size("...###more", [36.0, 32.0]) {
                    ui.open_popup("rule-actions");
                }
                if ui.is_item_hovered() {
                    ui.tooltip_text(rust_i18n::t!("rules.more"));
                }
                ui.popup("rule-actions", || {
                    if ui
                        .menu_item_config(rust_i18n::t!("rules.up"))
                        .enabled(i > 0)
                        .build()
                    {
                        swap = Some((i, i - 1));
                    }
                    if ui
                        .menu_item_config(rust_i18n::t!("rules.down"))
                        .enabled(i + 1 < count)
                        .build()
                    {
                        swap = Some((i, i + 1));
                    }
                    ui.separator();
                    if ui.menu_item(rust_i18n::t!("rules.remove")) {
                        delete = Some(i);
                    }
                });
            });
    }
    if let Some(i) = delete {
        app.rules.remove(i);
        app.request_rules_save();
    } else if let Some((a, b)) = swap {
        app.rules.swap(a, b);
        app.request_rules_save();
    }
}

pub fn editor(ui: &Ui, app: &mut App) {
    let Some(id) = app.rule_editor else {
        return;
    };
    let Some(draft) = app.rules.iter_mut().find(|draft| draft.id == id) else {
        app.rule_editor = None;
        return;
    };
    let _spacing = ui.push_style_var(StyleVar::ItemSpacing([10.0, 10.0]));
    let Some(_dialog) = dialog(ui, "rules.edit", "rule-editor", [640.0, 620.0]) else {
        return;
    };
    ui.set_window_font_scale(1.2);
    text(ui, "rules.edit");
    ui.set_window_font_scale(1.0);
    muted(ui, "rules.draft_hint");
    let mut edited = false;
    ui.child_window("rule-fields").size([0.0, -48.0]).build(|| {
        let _disabled = ui.begin_disabled(app.busy);
        edited = fields(ui, draft);
        if let Some(error) = &app.rule_save_error {
            ui.text_wrapped(error);
        }
    });
    ui.set_cursor_pos([ui.window_content_region_max()[0] - 96.0, ui.cursor_pos()[1]]);
    if primary_button(ui, label("action.close", "close-rule"), [96.0, 34.0])
        || ui.is_key_pressed(Key::Escape)
    {
        app.rule_editor = None;
        ui.close_current_popup();
        edited = true;
    }
    if edited {
        app.request_rules_save();
    }
}

fn fields(ui: &Ui, draft: &mut RuleDraft) -> bool {
    let mut edited = false;
    let _id = ui.push_id(draft.id.to_string());
    let columns = ui.content_region_avail()[0] >= 520.0;
    if columns {
        ui.columns(2, "rule-columns", false);
    }
    text(ui, "rules.match_group");
    edited |= field(
        ui,
        "rules.protocol",
        "protocol",
        &mut draft.rule.protocol,
        false,
    );
    edited |= field(ui, "rules.host", "host", &mut draft.rule.host, false);
    edited |= optional_field(
        ui,
        "rules.path",
        "path",
        &mut draft.rule.path,
        &mut draft.optional_literal_empty,
    );
    edited |= field(ui, "rules.port", "port", &mut draft.source_port, true);
    if columns {
        ui.next_column();
    } else {
        ui.dummy([0.0, 4.0]);
    }
    text(ui, "rules.target_group");
    edited |= optional_field(
        ui,
        "rules.target_protocol",
        "target-protocol",
        &mut draft.rule.target_protocol,
        &mut draft.optional_literal_empty,
    );
    edited |= field(
        ui,
        "rules.target_host",
        "target-host",
        &mut draft.rule.target_host,
        false,
    );
    edited |= field(
        ui,
        "rules.target_port",
        "target-port",
        &mut draft.target_port,
        true,
    );
    if columns {
        ui.columns(1, "rule-columns", false);
    }
    ui.dummy([0.0, 4.0]);
    text(ui, "rules.path_group");
    ui.separator();
    edited |= optional_field(
        ui,
        "rules.transform",
        "transform",
        &mut draft.rule.path_transform,
        &mut draft.optional_literal_empty,
    );
    edited |= optional_field(
        ui,
        "rules.target_path",
        "target-path",
        &mut draft.rule.target_path,
        &mut draft.optional_literal_empty,
    );
    edited
}
