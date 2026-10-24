use imgui::{Context, FontSource, MouseButton, internal::RawWrapper};
use proxy_fork_core::app_config::AppConfig;
use proxy_fork_gui::{
    app::{Action, App, Page},
    logging::LogBuffer,
    preferences::{Preferences, ThemePreference},
    view,
};

#[test]
fn sidebar_navigation_preserves_drafts_and_keeps_page_scroll_independent() {
    let mut context = Context::create();
    context.set_ini_filename(None);
    context.io_mut().display_size = [1000.0, 720.0];
    context.io_mut().delta_time = 1.0 / 60.0;
    context
        .fonts()
        .add_font(&[FontSource::DefaultFontData { config: None }]);
    context.fonts().build_rgba32_texture();
    let mut app = App::new(AppConfig::default(), Preferences::default());
    app.set_locale("en");
    app.add_rule();
    app.rules[0].rule.host = "draft.example".into();
    let draft_id = app.rules[0].id;
    let logs = LogBuffer::default();
    // A blank header click must not submit configuration: edits save themselves.
    context.io_mut().add_mouse_pos_event([850.0, 34.0]);
    for _ in 0..4 {
        view::draw(
            context.frame(),
            &mut app,
            &logs,
            winit::window::Theme::Light,
        );
        context.render();
    }
    context
        .io_mut()
        .add_mouse_button_event(MouseButton::Left, true);
    context
        .io_mut()
        .add_mouse_button_event(MouseButton::Left, false);
    let mut header_actions = Vec::new();
    for _ in 0..4 {
        header_actions.extend(view::draw(
            context.frame(),
            &mut app,
            &logs,
            winit::window::Theme::Light,
        ));
        context.render();
    }
    assert!(
        header_actions.is_empty(),
        "Overview must not have a Save & apply button"
    );
    let frame = |context: &mut Context, app: &mut App| {
        let actions = view::draw(context.frame(), app, &logs, winit::window::Theme::Light);
        context.render();
        assert!(
            actions.is_empty()
                || (app.page == Page::Settings && actions == vec![Action::InspectAutostart])
        );
    };
    for _ in 0..4 {
        frame(&mut context, &mut app);
    }
    for (page, position) in [
        (Page::Rules, [80.0, 142.0]),
        (Page::Logs, [80.0, 192.0]),
        (Page::Settings, [80.0, 242.0]),
        (Page::Dashboard, [80.0, 92.0]),
    ] {
        context.io_mut().add_mouse_pos_event(position);
        context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, true);
        context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, false);
        frame(&mut context, &mut app);
        frame(&mut context, &mut app);
        assert_eq!(app.page, page, "sidebar click should navigate after idle");
        assert_eq!(app.rules[0].id, draft_id);
        assert_eq!(app.rules[0].rule.host, "draft.example");
    }

    context.io_mut().display_size = [940.0, 500.0];
    app.page = Page::Settings;
    for _ in 0..4 {
        frame(&mut context, &mut app);
    }
    context.io_mut().add_mouse_pos_event([800.0, 350.0]);
    context.io_mut().add_mouse_wheel_event([0.0, -10.0]);
    for _ in 0..4 {
        frame(&mut context, &mut app);
    }
    app.page = Page::Dashboard;
    for _ in 0..4 {
        frame(&mut context, &mut app);
    }
    context.io_mut().add_mouse_pos_event([280.0, 255.0]);
    context
        .io_mut()
        .add_mouse_button_event(MouseButton::Left, true);
    context
        .io_mut()
        .add_mouse_button_event(MouseButton::Left, false);
    let mut actions = Vec::new();
    for _ in 0..4 {
        actions.extend(view::draw(
            context.frame(),
            &mut app,
            &logs,
            winit::window::Theme::Light,
        ));
        context.render();
    }
    assert_eq!(
        actions,
        vec![Action::Start],
        "scrolling Settings must not move Overview's Start button"
    );

    let click = |context: &mut Context, app: &mut App, position| {
        context.io_mut().add_mouse_pos_event(position);
        context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, true);
        context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, false);
        let mut actions = Vec::new();
        for _ in 0..4 {
            actions.extend(view::draw(
                context.frame(),
                app,
                &logs,
                winit::window::Theme::Light,
            ));
            context.render();
        }
        actions
    };
    context.io_mut().display_size = [940.0, 700.0];
    for locale in ["en", "zh-CN"] {
        app.set_locale(locale);
        app.page = Page::Dashboard;
        for _ in 0..4 {
            frame(&mut context, &mut app);
        }
        assert!(
            click(&mut context, &mut app, [500.0, 255.0]).is_empty(),
            "empty space beside Start must not start the proxy"
        );
        assert!(click(&mut context, &mut app, [500.0, 550.0]).is_empty());
        assert_eq!(
            app.page,
            Page::Dashboard,
            "empty space beside Manage rules must not navigate"
        );
        assert!(click(&mut context, &mut app, [850.0, 550.0]).is_empty());
        assert!(
            !app.ca_dialog,
            "empty space beside Manage certificates must not inspect a CA"
        );
        assert_eq!(
            click(&mut context, &mut app, [280.0, 255.0]),
            vec![Action::Start]
        );
        assert!(click(&mut context, &mut app, [280.0, 550.0]).is_empty());
        assert_eq!(app.page, Page::Rules);
        app.page = Page::Dashboard;
        for _ in 0..4 {
            frame(&mut context, &mut app);
        }
        assert_eq!(
            click(&mut context, &mut app, [610.0, 535.0]),
            vec![Action::InspectCa],
            "certificate button should open in {locale}"
        );
        assert!(app.ca_dialog);
        app.ca_dialog = false;
    }

    for viewport in [[940.0, 700.0], [640.0, 480.0]] {
        context.io_mut().display_size = viewport;
        for locale in ["en", "zh-CN"] {
            app.set_locale(locale);
            app.page = Page::Dashboard;
            for _ in 0..4 {
                frame(&mut context, &mut app);
            }
            assert_eq!(
                click(&mut context, &mut app, [80.0, viewport[1] - 55.0]),
                Vec::<Action>::new(),
                "the sidebar footer must be informational in {locale} at {viewport:?}"
            );
        }
    }

    context.io_mut().display_size = [940.0, 700.0];
    app.page = Page::Settings;
    context.io_mut().add_mouse_pos_event([800.0, 350.0]);
    context.io_mut().add_mouse_wheel_event([0.0, 100.0]);
    for locale in ["en", "zh-CN"] {
        app.set_locale(locale);
        for (enabled, expected) in [
            (false, Action::EnableAutostart),
            (true, Action::DisableAutostart),
        ] {
            app.autostart = Some(enabled);
            for _ in 0..4 {
                frame(&mut context, &mut app);
            }
            assert_eq!(
                click(&mut context, &mut app, [521.0, 166.0]),
                if cfg!(target_os = "macos") {
                    vec![expected]
                } else {
                    Vec::new()
                },
                "the autostart switch must request the opposite of {enabled} in {locale}"
            );
            assert_eq!(
                app.autostart,
                Some(enabled),
                "the switch must wait for the system result before changing state"
            );
        }
        app.busy = true;
        for _ in 0..4 {
            frame(&mut context, &mut app);
        }
        assert!(click(&mut context, &mut app, [521.0, 189.0]).is_empty());
        app.busy = false;
        app.autostart = None;
        for _ in 0..4 {
            frame(&mut context, &mut app);
        }
        assert_eq!(
            click(&mut context, &mut app, [521.0, 166.0]),
            if cfg!(target_os = "macos") {
                vec![Action::InspectAutostart]
            } else {
                Vec::new()
            },
            "an unknown autostart state must be checked before changing it"
        );
    }

    if cfg!(target_os = "macos") {
        app.error = Some("registered application moved; enable autostart again".into());
        for locale in ["en", "zh-CN"] {
            app.set_locale(locale);
            for _ in 0..4 {
                frame(&mut context, &mut app);
            }
            for (y, expected) in [
                (235.0, Action::InspectAutostart),
                (258.0, Action::EnableAutostart),
                (281.0, Action::DisableAutostart),
            ] {
                assert!(
                    click(&mut context, &mut app, [521.0, 212.0]).is_empty(),
                    "an inspection error must offer explicit recovery, not repeat the failed read"
                );
                assert_eq!(
                    click(&mut context, &mut app, [540.0, y]),
                    vec![expected],
                    "a stale login item must be recoverable in {locale}"
                );
                assert_eq!(app.autostart, None);
            }
        }
        app.error = None;
    }

    for (locale, appearance) in [
        ("en", winit::window::Theme::Light),
        ("en", winit::window::Theme::Dark),
        ("zh-CN", winit::window::Theme::Light),
        ("zh-CN", winit::window::Theme::Dark),
    ] {
        proxy_fork_gui::render::apply_theme(context.style_mut(), appearance);
        app.set_locale(locale);
        for _ in 0..4 {
            frame(&mut context, &mut app);
        }
        for (x, theme) in [
            (808.0, ThemePreference::Dark),
            (868.0, ThemePreference::System),
            (748.0, ThemePreference::Light),
        ] {
            app.preferences_dirty = false;
            assert!(click(&mut context, &mut app, [x, 214.0]).is_empty());
            assert_eq!(app.preferences.theme, theme, "theme segment in {locale}");
            assert!(
                app.preferences_dirty,
                "theme changes must queue an automatic save"
            );
            context.io_mut().add_mouse_pos_event([600.0, 100.0]);
            frame(&mut context, &mut app);
            assert!(view::draw(context.frame(), &mut app, &logs, appearance).is_empty());
            let vertices: Vec<_> = context
                .render()
                .draw_lists()
                .flat_map(|list| list.vtx_buffer())
                .filter(|vertex| {
                    vertex.col == imgui::ImColor32::from(view::ACCENT).to_rgba()
                        && vertex.pos[0] > 700.0
                        && (190.0..240.0).contains(&vertex.pos[1])
                })
                .map(|vertex| vertex.pos)
                .collect();
            assert!(
                !vertices.is_empty(),
                "the selected theme must remain visible"
            );
            let left = vertices.iter().map(|v| v[0]).fold(f32::INFINITY, f32::min);
            let top = vertices.iter().map(|v| v[1]).fold(f32::INFINITY, f32::min);
            assert!(
                !vertices
                    .iter()
                    .any(|v| v[0] < left + 1.0 && v[1] < top + 1.0),
                "the selected theme must have a rounded corner in {locale}: {theme:?}"
            );
        }
        assert!(click(&mut context, &mut app, [839.0, 214.0]).is_empty());
        assert_eq!(
            app.preferences.theme,
            ThemePreference::Light,
            "the gap must not switch themes"
        );
        let automatic_updates = app.preferences.automatic_updates;
        app.preferences_dirty = false;
        assert!(click(&mut context, &mut app, [521.0, 310.0]).is_empty());
        assert_eq!(
            app.preferences.automatic_updates,
            automatic_updates ^ cfg!(target_os = "macos"),
            "the update switch must preserve the platform guard in {locale}"
        );
        assert_eq!(app.preferences_dirty, cfg!(target_os = "macos"));
        assert!(
            click(&mut context, &mut app, [824.0, 270.0]).is_empty(),
            "Settings must not have a Save preferences button in {locale}"
        );
        assert_eq!(
            click(&mut context, &mut app, [467.0, 358.0]),
            if cfg!(target_os = "macos") {
                vec![Action::CheckUpdate]
            } else {
                Vec::new()
            },
            "manual update check must remain reachable in {locale}"
        );
        assert_eq!(app.update_dialog, cfg!(target_os = "macos"));
        app.update_dialog = false;
        app.busy = true;
        for _ in 0..4 {
            frame(&mut context, &mut app);
        }
        let theme = app.preferences.theme;
        let automatic_updates = app.preferences.automatic_updates;
        app.preferences_dirty = false;
        assert!(click(&mut context, &mut app, [808.0, 214.0]).is_empty());
        assert!(click(&mut context, &mut app, [521.0, 310.0]).is_empty());
        assert_eq!(
            app.preferences.theme, theme,
            "busy preferences must be frozen"
        );
        assert_eq!(app.preferences.automatic_updates, automatic_updates);
        assert!(!app.preferences_dirty);
        assert!(click(&mut context, &mut app, [824.0, 293.0]).is_empty());
        assert!(click(&mut context, &mut app, [467.0, 381.0]).is_empty());
        assert!(!app.update_dialog);
        app.busy = false;
        app.exiting = true;
        for _ in 0..4 {
            frame(&mut context, &mut app);
        }
        assert!(click(&mut context, &mut app, [808.0, 214.0]).is_empty());
        assert!(click(&mut context, &mut app, [521.0, 310.0]).is_empty());
        assert_eq!(
            app.preferences.theme, theme,
            "exit must not lose late edits"
        );
        assert_eq!(app.preferences.automatic_updates, automatic_updates);
        assert!(!app.preferences_dirty);
        app.exiting = false;
    }

    proxy_fork_gui::render::apply_theme(context.style_mut(), winit::window::Theme::Light);
    app.set_locale("en");
    for _ in 0..4 {
        frame(&mut context, &mut app);
    }
    for (y, locale) in [(224.0, "zh-CN"), (201.0, "en")] {
        assert!(click(&mut context, &mut app, [820.0, 166.0]).is_empty());
        let actions = click(&mut context, &mut app, [790.0, y]);
        assert_eq!(
            app.preferences.locale, locale,
            "language dropdown selection"
        );
        assert!(
            actions.is_empty(),
            "changing language must not repeat system inspection"
        );
    }

    context.io_mut().display_size = [640.0, 760.0];
    for locale in ["en", "zh-CN"] {
        app.set_locale(locale);
        app.autostart = Some(false);
        for _ in 0..4 {
            frame(&mut context, &mut app);
        }
        assert_eq!(
            click(&mut context, &mut app, [577.0, 166.0]),
            if cfg!(target_os = "macos") {
                vec![Action::EnableAutostart]
            } else {
                Vec::new()
            },
            "the system card must reflow within the narrow viewport in {locale}"
        );
        assert!(click(&mut context, &mut app, [508.0, 358.0]).is_empty());
        assert_eq!(app.preferences.theme, ThemePreference::Dark);
    }
    context.io_mut().display_size = [940.0, 700.0];

    app.page = Page::Dashboard;
    for _ in 0..4 {
        frame(&mut context, &mut app);
    }
    app.page = Page::Settings;
    let actions = view::draw(
        context.frame(),
        &mut app,
        &logs,
        winit::window::Theme::Light,
    );
    context.render();
    assert_eq!(
        actions,
        if cfg!(target_os = "macos") {
            vec![Action::InspectAutostart]
        } else {
            Vec::new()
        },
        "entering Settings must inspect the current system state"
    );
    for _ in 0..4 {
        let actions = view::draw(
            context.frame(),
            &mut app,
            &logs,
            winit::window::Theme::Light,
        );
        context.render();
        assert!(
            actions.is_empty(),
            "an unknown state must not cause a retry loop"
        );
    }

    app.preferences_dirty = false;
    app.page = Page::Rules;
    app.add_rule();
    app.rules[1].rule.host = "second.example".into();
    app.rules_saved(app.edited_rules().unwrap());
    for _ in 0..4 {
        frame(&mut context, &mut app);
    }
    let before = app.rules.len();
    assert!(click(&mut context, &mut app, [270.0, 120.0]).is_empty());
    assert_eq!(
        app.rules.len(),
        before + 1,
        "the compact Add action opens a new draft"
    );
    assert!(
        unsafe {
            imgui::sys::igIsPopupOpen(
                std::ptr::null(),
                (imgui::sys::ImGuiPopupFlags_AnyPopupId | imgui::sys::ImGuiPopupFlags_AnyPopupLevel)
                    as i32,
            )
        },
        "adding a rule must open its grouped editor instead of another inline form"
    );
    let new_id = app.rules.last().unwrap().id;
    assert_eq!(app.rule_editor, Some(new_id));
    assert_eq!(app.rules.last().unwrap().rule.path, None);
    assert_eq!(app.rules.last().unwrap().rule.target_path, None);
    assert_eq!(app.rules.last().unwrap().rule.target_protocol, None);
    assert_eq!(app.queued_action(), None, "an empty new draft is not saved");
    assert!(click(&mut context, &mut app, [350.0, 215.0]).is_empty());
    for character in "new.example".chars() {
        context.io_mut().add_input_character(character);
    }
    for _ in 0..4 {
        frame(&mut context, &mut app);
    }
    assert_eq!(
        app.rules.last().unwrap().rule.host,
        "new.example",
        "editor text goes to the selected draft"
    );
    assert_eq!(
        app.queued_action(),
        None,
        "typing alone must not write each character"
    );
    assert!(click(&mut context, &mut app, [600.0, 169.0]).is_empty());
    assert_eq!(app.queued_action(), Some(Action::SaveRules));
    assert_eq!(app.config.proxy_manager.rules.len(), 2);
    app.rules_saved(app.edited_rules().unwrap());
    for character in "http".chars() {
        context.io_mut().add_input_character(character);
    }
    for _ in 0..4 {
        frame(&mut context, &mut app);
    }
    assert_eq!(
        app.rules.last().unwrap().rule.target_protocol.as_deref(),
        Some("http"),
        "Target's first field must align with Source's first field"
    );
    assert!(click(&mut context, &mut app, [738.0, 623.0]).is_empty());
    assert_eq!(
        app.rule_editor, None,
        "Close stays reachable below the scroll body"
    );
    assert_eq!(app.rules.last().unwrap().rule.host, "new.example");
    assert_eq!(app.queued_action(), Some(Action::SaveRules));
    assert_eq!(app.config.proxy_manager.rules[2].target_protocol, None);
    app.rules_saved(app.edited_rules().unwrap());
    assert_eq!(app.queued_action(), None);

    app.rules[2].rule.path = Some(String::new());
    app.rules[2].rule.path_transform = Some("replace".into());
    app.rules[2].rule.target_path = Some(String::new());
    app.rules_saved(app.edited_rules().unwrap());
    app.rule_editor = Some(new_id);
    for _ in 0..4 {
        frame(&mut context, &mut app);
    }
    assert_eq!(app.rules[2].rule.path.as_deref(), Some(""));
    assert_eq!(app.rules[2].rule.target_path.as_deref(), Some(""));
    assert!(click(&mut context, &mut app, [738.0, 623.0]).is_empty());
    assert_eq!(
        app.queued_action(),
        None,
        "opening and closing is not an edit"
    );
    app.rule_editor = Some(new_id);
    for _ in 0..4 {
        frame(&mut context, &mut app);
    }
    assert!(click(&mut context, &mut app, [350.0, 268.0]).is_empty());
    context.io_mut().add_input_character('X');
    for _ in 0..4 {
        frame(&mut context, &mut app);
    }
    assert_eq!(app.rules[2].rule.path.as_deref(), Some("X"));
    context.io_mut().add_key_event(imgui::Key::Backspace, true);
    frame(&mut context, &mut app);
    context.io_mut().add_key_event(imgui::Key::Backspace, false);
    for _ in 0..4 {
        frame(&mut context, &mut app);
    }
    assert_eq!(
        app.rules[2].rule.path, None,
        "clearing removes an optional match"
    );
    context.io_mut().add_key_event(imgui::Key::Escape, true);
    frame(&mut context, &mut app);
    context.io_mut().add_key_event(imgui::Key::Escape, false);
    for _ in 0..4 {
        frame(&mut context, &mut app);
    }
    assert_eq!(app.rule_editor, None);
    assert_eq!(app.rules[2].rule.path.as_deref(), Some(""));
    assert_eq!(app.edited_rules().unwrap()[2].path.as_deref(), Some(""));
    assert_eq!(app.queued_action(), None);

    app.rules.truncate(2);
    app.rules[1].rule.host = format!("{}example", "very-long-label.".repeat(24));
    let second_id = app.rules[1].id;
    for locale in ["en", "zh-CN"] {
        app.set_locale(locale);
        for _ in 0..4 {
            frame(&mut context, &mut app);
        }
        assert!(click(&mut context, &mut app, [840.0, 274.0]).is_empty());
        assert_eq!(
            app.rule_editor,
            Some(second_id),
            "the compact second row must expose Edit in {locale}"
        );
        app.rules.swap(0, 1);
        for _ in 0..4 {
            frame(&mut context, &mut app);
        }
        assert_eq!(
            app.rule_editor,
            Some(second_id),
            "editor identity must survive reordering"
        );
        assert!(click(&mut context, &mut app, [738.0, 623.0]).is_empty());
        assert_eq!(app.rule_editor, None);
        app.rules.swap(0, 1);
        app.busy = true;
        let count = app.rules.len();
        for _ in 0..4 {
            frame(&mut context, &mut app);
        }
        assert!(click(&mut context, &mut app, [270.0, 143.0]).is_empty());
        assert_eq!(app.rules.len(), count, "busy Add must not mutate rules");
        app.busy = false;
        for _ in 0..4 {
            frame(&mut context, &mut app);
        }
        assert!(
            click(&mut context, &mut app, [825.0, 120.0]).is_empty(),
            "the Rules page must not expose a separate Save action"
        );
    }

    for locale in ["en", "zh-CN"] {
        context.io_mut().display_size = [640.0, 480.0];
        app.set_locale(locale);
        app.rule_editor = Some(second_id);
        for _ in 0..4 {
            frame(&mut context, &mut app);
        }
        assert!(click(&mut context, &mut app, [553.0, 425.0]).is_empty());
        assert_eq!(
            app.rule_editor, None,
            "editor Close must fit the minimum viewport in {locale}"
        );
    }

    context.io_mut().display_size = [940.0, 700.0];
    app.ca_dialog = true;
    app.ca_missing = true;
    for _ in 0..4 {
        frame(&mut context, &mut app);
    }
    assert!(
        unsafe {
            imgui::sys::igIsPopupOpen(
                std::ptr::null(),
                (imgui::sys::ImGuiPopupFlags_AnyPopupId | imgui::sys::ImGuiPopupFlags_AnyPopupLevel)
                    as i32,
            )
        },
        "certificate management should be a centered modal, not an independent floating window"
    );
    assert_eq!(
        click(&mut context, &mut app, [250.0, 200.0]),
        vec![Action::GenerateCa],
        "missing CA retains an explicit generation action"
    );
    assert_eq!(
        click(&mut context, &mut app, [706.0, 119.0]),
        vec![Action::InspectCa]
    );
    assert!(click(&mut context, &mut app, [700.0, 630.0]).is_empty());
    assert!(!app.ca_dialog);

    for locale in ["en", "zh-CN"] {
        app.set_locale(locale);
        app.ca_dialog = true;
        app.ca_missing = false;
        app.ca_fingerprint = Some("0123456789abcdef".repeat(4));
        app.ca_keychain = Some("/test/login.keychain-db".into());
        app.ca_trusted = Some(false);
        app.preferences.trusted_fingerprint = app.ca_fingerprint.clone();
        let offset = if locale == "en" { 0.0 } else { -13.0 };
        for _ in 0..4 {
            frame(&mut context, &mut app);
        }
        context.io_mut().add_mouse_pos_event([650.0, 530.0]);
        context.io_mut().add_mouse_wheel_event([0.0, 100.0]);
        for _ in 0..4 {
            frame(&mut context, &mut app);
        }
        assert!(
            click(&mut context, &mut app, [250.0, 515.0 + offset]).is_empty(),
            "trust requires explicit authorization in {locale}"
        );
        assert!(click(&mut context, &mut app, [200.0, 480.0 + offset]).is_empty());
        assert!(app.ca_confirm, "authorization checkbox in {locale}");
        assert_eq!(
            click(&mut context, &mut app, [250.0, 515.0 + offset]),
            if cfg!(target_os = "macos") {
                vec![Action::TrustCa]
            } else {
                Vec::new()
            }
        );
        app.ca_keychain = None;
        assert!(
            click(&mut context, &mut app, [250.0, 515.0 + offset]).is_empty(),
            "a resolved Keychain is required"
        );
        app.ca_keychain = Some("/test/login.keychain-db".into());
        if locale == "en" {
            // Other platforms show an extra unsupported-feature hint above this section.
            let danger_y = if cfg!(target_os = "macos") {
                568.0
            } else {
                591.0
            };
            assert!(click(&mut context, &mut app, [250.0, danger_y]).is_empty());
        }
        context.io_mut().add_mouse_pos_event([650.0, 530.0]);
        context.io_mut().add_mouse_wheel_event([0.0, -30.0]);
        for _ in 0..4 {
            frame(&mut context, &mut app);
        }
        app.preferences.trusted_fingerprint = None;
        assert!(click(&mut context, &mut app, [200.0, 510.0]).is_empty());
        assert!(app.ca_overwrite, "overwriting remains an explicit checkbox");
        assert_eq!(
            click(&mut context, &mut app, [250.0, 545.0]),
            vec![Action::GenerateCa]
        );
        app.preferences.trusted_fingerprint = app.ca_fingerprint.clone();
        assert_eq!(
            click(&mut context, &mut app, [240.0, 585.0]),
            if cfg!(target_os = "macos") {
                vec![Action::RemoveCa]
            } else {
                Vec::new()
            },
            "removal stays reachable in the separate danger section"
        );
        app.preferences.trusted_fingerprint = Some("another certificate".into());
        assert!(
            click(&mut context, &mut app, [240.0, 585.0]).is_empty(),
            "never remove an unowned fingerprint"
        );
        app.preferences.trusted_fingerprint = app.ca_fingerprint.clone();
        app.ca_confirm = false;
        assert!(
            click(&mut context, &mut app, [240.0, 585.0]).is_empty(),
            "removal still requires explicit authorization"
        );
        app.ca_confirm = true;
        app.busy = true;
        for _ in 0..4 {
            frame(&mut context, &mut app);
        }
        assert!(click(&mut context, &mut app, [240.0, 585.0]).is_empty());
        assert!(click(&mut context, &mut app, [250.0, 545.0]).is_empty());
        assert!(click(&mut context, &mut app, [700.0, 630.0]).is_empty());
        assert!(!app.ca_dialog);
        assert!(
            !app.ca_confirm && !app.ca_overwrite,
            "closing revokes dialog consent"
        );
        app.busy = false;
    }

    for locale in ["en", "zh-CN"] {
        app.set_locale(locale);
        context.io_mut().display_size = [640.0, 480.0];
        app.ca_dialog = true;
        app.cert_path = format!("/test/{}/ca.pem", "long-directory-name/".repeat(12));
        for _ in 0..4 {
            frame(&mut context, &mut app);
        }
        assert!(click(&mut context, &mut app, [553.0, 425.0]).is_empty());
        assert!(
            !app.ca_dialog,
            "CA Close must remain reachable despite long paths and scrolling in {locale}"
        );
    }

    context.io_mut().display_size = [940.0, 700.0];
    app.set_locale("en");
    for _ in 0..4 {
        frame(&mut context, &mut app);
    }
    app.rules_saved(app.edited_rules().unwrap());
    assert!(click(&mut context, &mut app, [884.0, 190.0]).is_empty());
    assert!(click(&mut context, &mut app, [870.0, 212.0]).is_empty());
    assert_ne!(
        app.rules[0].id, second_id,
        "Move up is disabled for the first row"
    );
    assert!(click(&mut context, &mut app, [870.0, 235.0]).is_empty());
    assert_eq!(app.queued_action(), Some(Action::SaveRules));
    app.rules_saved(app.edited_rules().unwrap());
    assert_eq!(
        app.rules[0].id, second_id,
        "More > Move down must preserve the moved draft's identity"
    );
    assert!(click(&mut context, &mut app, [884.0, 274.0]).is_empty());
    assert!(click(&mut context, &mut app, [870.0, 345.0]).is_empty());
    assert_eq!(app.queued_action(), Some(Action::SaveRules));
    app.rules_saved(app.edited_rules().unwrap());
    assert_eq!(
        app.rules.len(),
        1,
        "More > Delete removes only the chosen row"
    );
    assert_eq!(app.rules[0].id, second_id);

    app.rule_editor = Some(second_id);
    app.busy = true;
    let host = app.rules[0].rule.host.clone();
    for _ in 0..4 {
        frame(&mut context, &mut app);
    }
    assert!(click(&mut context, &mut app, [350.0, 215.0]).is_empty());
    context.io_mut().add_input_character('X');
    for _ in 0..4 {
        frame(&mut context, &mut app);
    }
    assert_eq!(
        app.rules[0].rule.host, host,
        "busy editors cannot change draft fields"
    );
    assert!(click(&mut context, &mut app, [738.0, 623.0]).is_empty());
    assert_eq!(app.rule_editor, None);
    app.busy = false;

    app.rules_saved(app.edited_rules().unwrap());
    app.rule_editor = Some(second_id);
    for _ in 0..4 {
        frame(&mut context, &mut app);
    }
    assert!(click(&mut context, &mut app, [350.0, 228.0]).is_empty());
    let original = app.rules[0].rule.host.clone();
    context.io_mut().add_input_character('X');
    for _ in 0..4 {
        frame(&mut context, &mut app);
    }
    assert_ne!(app.rules[0].rule.host, original);
    context.io_mut().add_key_event(imgui::Key::Escape, true);
    frame(&mut context, &mut app);
    context.io_mut().add_key_event(imgui::Key::Escape, false);
    for _ in 0..4 {
        frame(&mut context, &mut app);
    }
    assert_eq!(app.rule_editor, None);
    assert_eq!(
        app.rules[0].rule.host, original,
        "Escape cancels the active field"
    );
    assert_eq!(app.queued_action(), None);

    context.fonts().clear();
    context.fonts().add_font(&[FontSource::TtfData {
        data: include_bytes!("../assets/fonts/NotoSansSC.ttf"),
        size_pixels: 18.0,
        config: Some(imgui::FontConfig {
            oversample_h: 1,
            oversample_v: 1,
            glyph_ranges: imgui::FontGlyphRanges::chinese_simplified_common(),
            ..imgui::FontConfig::default()
        }),
    }]);
    context.fonts().build_rgba32_texture();
    context.style_mut().frame_padding = [10.0, 6.0];
    context.io_mut().display_size = [940.0, 700.0];
    for locale in ["en", "zh-CN"] {
        app.set_locale(locale);
        app.page = Page::Rules;
        for _ in 0..4 {
            frame(&mut context, &mut app);
        }
        assert!(click(&mut context, &mut app, [775.0, 203.0]).is_empty());
        assert_eq!(
            app.rule_editor,
            Some(second_id),
            "Edit needs space for the bundled 18px font and native padding in {locale}"
        );
        assert!(click(&mut context, &mut app, [738.0, 623.0]).is_empty());
        assert_eq!(app.rule_editor, None);
    }
    context.io_mut().display_size = [640.0, 480.0];
    for locale in ["en", "zh-CN"] {
        app.set_locale(locale);
        app.rule_editor = Some(second_id);
        for _ in 0..4 {
            frame(&mut context, &mut app);
        }
        assert!(click(&mut context, &mut app, [553.0, 425.0]).is_empty());
        assert_eq!(
            app.rule_editor, None,
            "Close must fit with native-sized text in {locale}"
        );
        app.ca_dialog = true;
        for _ in 0..4 {
            frame(&mut context, &mut app);
        }
        assert!(click(&mut context, &mut app, [553.0, 425.0]).is_empty());
        assert!(
            !app.ca_dialog,
            "CA Close must fit with native-sized text in {locale}"
        );
    }

    let tooltip_frame = |context: &mut Context, app: &mut App| {
        assert!(view::draw(context.frame(), app, &logs, winit::window::Theme::Light).is_empty());
        context.render().draw_lists().find_map(|list| {
            // The owning window name remains valid while this frame's draw data is borrowed.
            let name = unsafe { list.raw()._OwnerName };
            if name.is_null()
                || !unsafe { std::ffi::CStr::from_ptr(name) }
                    .to_bytes()
                    .starts_with(b"##Tooltip_")
            {
                return None;
            }
            let mut bounds = [
                f32::INFINITY,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
            ];
            for vertex in list.vtx_buffer() {
                bounds[0] = bounds[0].min(vertex.pos[0]);
                bounds[1] = bounds[1].min(vertex.pos[1]);
                bounds[2] = bounds[2].max(vertex.pos[0]);
                bounds[3] = bounds[3].max(vertex.pos[1]);
            }
            Some(bounds)
        })
    };
    app.page = Page::Rules;
    for viewport in [[940.0, 700.0], [640.0, 480.0]] {
        context.io_mut().display_size = viewport;
        for locale in ["en", "zh-CN"] {
            app.set_locale(locale);
            for (host, path) in [
                (String::new(), None),
                (
                    format!("{}example.test", "very-long-label.".repeat(20)),
                    None,
                ),
                ("example.test".into(), Some("/中文/路径".repeat(12))),
            ] {
                let draft = &mut app.rules[0];
                draft.rule.protocol = "https".into();
                draft.rule.host = host;
                draft.rule.path = path;
                draft.source_port.clear();
                draft.rule.target_protocol = Some("http".into());
                draft.rule.target_host = "127.0.0.1".into();
                draft.rule.target_path = None;
                draft.target_port.clear();
                context.io_mut().add_mouse_pos_event([350.0, 190.0]);
                let mut bounds = None;
                for _ in 0..6 {
                    bounds = tooltip_frame(&mut context, &mut app);
                }
                let bounds = bounds.expect("hovering a rule summary must show its full endpoints");
                let width = bounds[2] - bounds[0];
                let height = bounds[3] - bounds[1];
                assert!(
                    (120.0..=480.0).contains(&width),
                    "tooltip must have a readable bounded width, not one character per line: {bounds:?} in {locale} at {viewport:?}"
                );
                assert!(
                    height <= viewport[1] - 32.0,
                    "tooltip must not become a tall strip: {bounds:?}"
                );
                assert!(bounds[0] >= -1.0 && bounds[2] <= viewport[0] + 1.0);
                assert!(bounds[1] >= -1.0 && bounds[3] <= viewport[1] + 1.0);
            }
        }
    }
}
