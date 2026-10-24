use super::{dialog, label, muted, primary_button, state_key, text};
use crate::app::{Action, App, Page};
use imgui::{Key, StyleColor, StyleVar, Ui};
use proxy_fork_core::proxy_service::ProxyServiceState;

fn card_layout(width: f32) -> [[f32; 4]; 4] {
    let columns = if width >= 2.0 * 320.0 + 16.0 { 2 } else { 1 };
    let card_width = (width - 16.0 * (columns - 1) as f32) / columns as f32;
    let mut top = 0.0;
    std::array::from_fn(|index| {
        let height = if index < 2 { 204.0 } else { 264.0 };
        let rect = [
            (index % columns) as f32 * (card_width + 16.0),
            top,
            card_width,
            height,
        ];
        if index % columns == columns - 1 {
            top += height + 16.0;
        }
        rect
    })
}

fn service(ui: &Ui, app: &App, actions: &mut Vec<Action>, theme: winit::window::Theme) {
    ui.set_cursor_pos([18.0, 62.0]);
    ui.set_window_font_scale(1.5);
    ui.text_colored(
        if app.running() {
            if theme == winit::window::Theme::Dark {
                [0.39, 0.83, 0.64, 1.0]
            } else {
                [0.12, 0.48, 0.34, 1.0]
            }
        } else {
            ui.style_color(StyleColor::Text)
        },
        rust_i18n::t!(state_key(app)),
    );
    ui.set_window_font_scale(1.0);
    ui.child_window("service-detail")
        .size([0.0, 44.0])
        .build(|| match &app.state {
            ProxyServiceState::Running(addr) => {
                ui.text_colored(ui.style_color(StyleColor::TextDisabled), addr.to_string())
            }
            ProxyServiceState::Failed(error) => ui.text_wrapped(error),
            _ => muted(ui, "dashboard.service_hint"),
        });
    ui.set_cursor_pos([18.0, 152.0]);
    let _disabled = ui.begin_disabled(app.busy);
    if primary_button(
        ui,
        label(
            if app.running() {
                "action.stop"
            } else {
                "action.start"
            },
            "toggle-proxy",
        ),
        [96.0, 34.0],
    ) {
        actions.push(if app.running() {
            Action::Stop
        } else {
            Action::Start
        });
    }
}

fn listener(ui: &Ui, app: &mut App) -> bool {
    ui.set_cursor_pos([18.0, 66.0]);
    ui.align_text_to_frame_padding();
    text(ui, "proxy.host");
    ui.same_line_with_pos(126.0);
    ui.set_next_item_width(-1.0);
    let mut changed = ui
        .input_text("###listen-host", &mut app.config.listen.host)
        .enter_returns_true(true)
        .build();
    changed |= ui.is_item_deactivated_after_edit();
    ui.set_cursor_pos([18.0, 110.0]);
    ui.align_text_to_frame_padding();
    text(ui, "proxy.port");
    ui.same_line_with_pos(126.0);
    ui.set_next_item_width(110.0);
    changed |= ui
        .input_text("###listen-port", &mut app.listen_port)
        .chars_decimal(true)
        .enter_returns_true(true)
        .build();
    changed |= ui.is_item_deactivated_after_edit();
    ui.set_cursor_pos([18.0, 158.0]);
    changed |= ui.checkbox(
        label("proxy.system", "system-proxy"),
        &mut app.config.enable_sysproxy,
    );
    changed
}

fn rules(ui: &Ui, app: &mut App) -> bool {
    ui.set_cursor_pos([18.0, 56.0]);
    ui.set_window_font_scale(1.8);
    ui.text(app.rules.len().to_string());
    ui.set_window_font_scale(1.0);
    muted(ui, "dashboard.rule_count");
    ui.set_cursor_pos([18.0, 132.0]);
    text(ui, "proxy.cache");
    ui.set_next_item_width(120.0);
    let mut changed = ui
        .input_text("###cache-size", &mut app.cache_size)
        .chars_decimal(true)
        .enter_returns_true(true)
        .build();
    changed |= ui.is_item_deactivated_after_edit();
    ui.set_cursor_pos([18.0, 212.0]);
    if ui.button_with_size(label("dashboard.manage_rules", "manage-rules"), [0.0, 34.0]) {
        app.page = Page::Rules;
    }
    changed
}

fn certificates(ui: &Ui, app: &mut App, actions: &mut Vec<Action>) -> bool {
    let _spacing = ui.push_style_var(StyleVar::ItemSpacing([10.0, 6.0]));
    ui.set_cursor_pos([18.0, 58.0]);
    let mut changed = ui.checkbox(
        label("dashboard.intercept", "enable-ca"),
        &mut app.config.enable_ca,
    );
    ui.set_cursor_pos([18.0, 94.0]);
    text(ui, "dashboard.cert");
    ui.set_next_item_width(-1.0);
    changed |= ui
        .input_text("###cert-path", &mut app.cert_path)
        .hint(rust_i18n::t!("dashboard.default_path"))
        .enter_returns_true(true)
        .build();
    changed |= ui.is_item_deactivated_after_edit();
    ui.set_cursor_pos([18.0, 150.0]);
    text(ui, "dashboard.key");
    ui.set_next_item_width(-1.0);
    changed |= ui
        .input_text("###key-path", &mut app.key_path)
        .hint(rust_i18n::t!("dashboard.default_path"))
        .enter_returns_true(true)
        .build();
    changed |= ui.is_item_deactivated_after_edit();
    ui.set_cursor_pos([18.0, 212.0]);
    if ui.button_with_size(label("dashboard.manage_ca", "open-ca"), [0.0, 34.0]) {
        app.ca_dialog = true;
        app.ca_confirm = false;
        actions.push(Action::InspectCa);
    }
    changed
}

pub fn draw(ui: &Ui, app: &mut App, actions: &mut Vec<Action>, theme: winit::window::Theme) {
    let origin = ui.cursor_pos();
    let cards = card_layout(ui.content_region_avail()[0]);
    let mut changed = false;
    for (index, title) in [
        "dashboard.service",
        "dashboard.listener",
        "dashboard.rules",
        "dashboard.https",
    ]
    .into_iter()
    .enumerate()
    {
        let [x, y, width, height] = cards[index];
        ui.set_cursor_pos([origin[0] + x, origin[1] + y]);
        let _padding = ui.push_style_var(StyleVar::WindowPadding([18.0, 16.0]));
        let _rounding = ui.push_style_var(StyleVar::ChildRounding(10.0));
        let _background =
            ui.push_style_color(StyleColor::ChildBg, ui.style_color(StyleColor::PopupBg));
        ui.child_window(format!("dashboard-card-{index}"))
            .size([width, height])
            .border(true)
            .scrollable(false)
            .build(|| {
                ui.set_window_font_scale(1.05);
                text(ui, title);
                ui.set_window_font_scale(1.0);
                ui.separator();
                let _disabled = ui.begin_disabled(app.busy);
                match index {
                    0 => service(ui, app, actions, theme),
                    1 => changed |= listener(ui, app),
                    2 => changed |= rules(ui, app),
                    _ => changed |= certificates(ui, app, actions),
                }
            });
    }
    if changed {
        app.request_config_save();
    }
    let bottom = cards.iter().map(|r| r[1] + r[3]).fold(0.0, f32::max);
    ui.set_cursor_pos([origin[0], origin[1] + bottom + 16.0]);
    if app.restart_required {
        text(ui, "proxy.restart");
    }
    muted(ui, "proxy.autosave");
}

pub fn certificate_dialog(ui: &Ui, app: &mut App, actions: &mut Vec<Action>) {
    if !app.ca_dialog {
        return;
    }
    let _spacing = ui.push_style_var(StyleVar::ItemSpacing([10.0, 10.0]));
    let Some(_dialog) = dialog(ui, "proxy.wizard", "ca-dialog", [608.0, 640.0]) else {
        return;
    };
    ui.set_window_font_scale(1.2);
    text(ui, "proxy.wizard");
    ui.set_window_font_scale(1.0);
    muted(ui, "ca.description");
    ui.child_window("ca-sections").size([0.0, -48.0]).build(|| {
        if let Some(error) = &app.error {
            text(ui, "state.error");
            ui.text_wrapped(error);
        }
        let _disabled = ui.begin_disabled(app.busy);
        text(ui, "ca.status_group");
        ui.same_line_with_pos(ui.window_content_region_max()[0] - 88.0);
        if ui.button_with_size(label("action.refresh", "inspect-ca"), [88.0, 28.0]) {
            actions.push(Action::InspectCa);
        }
        text(
            ui,
            if app.ca_missing {
                "ca.not_configured"
            } else {
                match app.ca_trusted {
                    Some(true) => "ca.trusted",
                    Some(false) => "ca.untrusted",
                    None => "ca.unknown",
                }
            },
        );
        if app.ca_missing {
            muted(ui, "ca.setup_hint");
        }
        if app.ca_fingerprint.is_none()
            && primary_button(ui, label("ca.generate", "generate-ca"), [128.0, 34.0])
        {
            actions.push(Action::GenerateCa);
        }

        ui.dummy([0.0, 2.0]);
        text(ui, "ca.info_group");
        ui.separator();
        let default_path = rust_i18n::t!("dashboard.default_path");
        information(
            ui,
            "dashboard.cert",
            "ca-cert",
            if app.cert_path.is_empty() {
                &default_path
            } else {
                &app.cert_path
            },
        );
        information(
            ui,
            "dashboard.key",
            "ca-key",
            if app.key_path.is_empty() {
                &default_path
            } else {
                &app.key_path
            },
        );
        let unknown = rust_i18n::t!("ca.unknown");
        information(
            ui,
            "ca.fingerprint",
            "ca-fingerprint",
            app.ca_fingerprint.as_deref().unwrap_or(&unknown),
        );

        ui.dummy([0.0, 2.0]);
        text(ui, "ca.trust_group");
        ui.separator();
        information(
            ui,
            "ca.keychain",
            "ca-keychain",
            app.ca_keychain.as_deref().unwrap_or(&unknown),
        );
        ui.checkbox(label("ca.confirm", "confirm-ca"), &mut app.ca_confirm);
        {
            let _confirm = ui.begin_disabled(!app.can_trust_ca() || !cfg!(target_os = "macos"));
            if primary_button(ui, label("ca.install", "install-ca"), [128.0, 34.0]) {
                actions.push(Action::TrustCa);
            }
        }
        if !cfg!(target_os = "macos") {
            muted(ui, "settings.unsupported");
        }

        ui.dummy([0.0, 6.0]);
        if ui.collapsing_header(
            label("ca.danger_group", "ca-danger"),
            imgui::TreeNodeFlags::empty(),
        ) {
            muted(ui, "ca.danger_hint");
            ui.checkbox(label("ca.overwrite", "overwrite-ca"), &mut app.ca_overwrite);
            if ui.button_with_size(label("ca.regenerate", "regenerate-ca"), [0.0, 32.0]) {
                actions.push(Action::GenerateCa);
            }
            let _recorded = ui.begin_disabled(
                !app.can_trust_ca()
                    || !cfg!(target_os = "macos")
                    || app.preferences.trusted_fingerprint != app.ca_fingerprint,
            );
            let _danger = ui.push_style_color(StyleColor::Text, [0.86, 0.28, 0.30, 1.0]);
            if ui.button_with_size(label("ca.remove", "remove-ca"), [0.0, 32.0]) {
                actions.push(Action::RemoveCa);
            }
        }
    });
    ui.set_cursor_pos([ui.window_content_region_max()[0] - 96.0, ui.cursor_pos()[1]]);
    if ui.button_with_size(label("action.close", "close-ca"), [96.0, 34.0])
        || ui.is_key_pressed(Key::Escape)
    {
        app.ca_dialog = false;
        app.ca_confirm = false;
        app.ca_overwrite = false;
        ui.close_current_popup();
    }
}

fn information(ui: &Ui, key: &str, id: &str, value: &str) {
    muted(ui, key);
    ui.set_next_item_width(-1.0);
    let mut value = value.to_owned();
    ui.input_text(format!("###{id}"), &mut value)
        .read_only(true)
        .auto_select_all(true)
        .build();
    if ui.is_item_hovered() {
        ui.tooltip_text(value);
    }
}

#[cfg(test)]
mod tests {
    use super::card_layout;

    #[test]
    fn wide_cards_stay_in_two_columns_when_a_previous_page_had_a_scrollbar() {
        let cards = card_layout(686.0);
        assert_eq!(cards[0][1], cards[1][1]);
        assert!(cards[1][0] > cards[0][0]);
    }

    #[test]
    fn cards_reflow_without_clipping_or_overlap() {
        for width in [320.0, 396.0, 640.0, 656.0, 686.0, 688.0, 740.0, 1120.0] {
            let cards = card_layout(width);
            for (i, &[x, y, w, h]) in cards.iter().enumerate() {
                assert!(x >= 0.0 && x + w <= width, "card {i} clips at {width}");
                assert!(w >= 320.0 && h >= 204.0);
                for &[other_x, other_y, other_w, other_h] in &cards[i + 1..] {
                    assert!(
                        x + w <= other_x
                            || other_x + other_w <= x
                            || y + h <= other_y
                            || other_y + other_h <= y,
                        "overlapping cards at {width}"
                    );
                }
            }
            if width >= 656.0 {
                assert_eq!(cards[0][1], cards[1][1], "wide layouts need two columns");
                assert_eq!(cards[2][1], cards[3][1]);
                assert!(cards[1][0] > cards[0][0]);
            } else {
                assert!(cards.windows(2).all(|pair| pair[0][1] < pair[1][1]));
            }
        }
    }
}
