use imgui::{Condition, Context, FontSource, Key, MouseButton};
use proxy_fork_core::app_config::AppConfig;
use proxy_fork_gui::{
    app::{Action, App},
    preferences::Preferences,
    view::rules,
};

fn frame(context: &mut Context, app: &mut App) -> Vec<Action> {
    let size = context.io().display_size;
    let ui = context.frame();
    let mut actions = Vec::new();
    ui.window("import-test")
        .position([0.0, 0.0], Condition::Always)
        .size(size, Condition::Always)
        .title_bar(false)
        .build(|| rules::draw(ui, app));
    rules::import_dialog(ui, app, &mut actions);
    context.render();
    actions
}

fn click(context: &mut Context, app: &mut App, position: [f32; 2]) -> Vec<Action> {
    context.io_mut().add_mouse_pos_event(position);
    context
        .io_mut()
        .add_mouse_button_event(MouseButton::Left, true);
    context
        .io_mut()
        .add_mouse_button_event(MouseButton::Left, false);
    (0..4).flat_map(|_| frame(context, app)).collect()
}

#[test]
fn import_dialog_opens_submits_once_blocks_empty_or_busy_submissions_and_cancels() {
    let mut context = Context::create();
    context.set_ini_filename(None);
    context.io_mut().delta_time = 1.0 / 60.0;
    context
        .fonts()
        .add_font(&[FontSource::DefaultFontData { config: None }]);
    context.fonts().build_rgba32_texture();
    let mut app = App::new(AppConfig::default(), Preferences::default());
    for size in [[940.0, 700.0], [640.0, 480.0]] {
        context.io_mut().display_size = size;
        for locale in ["en", "zh-CN"] {
            app.set_locale(locale);
            for _ in 0..4 {
                frame(&mut context, &mut app);
            }
            assert!(click(&mut context, &mut app, [180.0, 40.0]).is_empty());
            assert!(app.rule_import_dialog, "Import button must open the dialog");
            let submit = [size[0] * 0.5 + 220.0, size[1] * 0.5 + 133.0];
            assert!(click(&mut context, &mut app, submit).is_empty());
            app.rule_import_source = "https://example.com/rules.toml".into();
            for _ in 0..4 {
                frame(&mut context, &mut app);
            }
            assert_eq!(
                click(&mut context, &mut app, submit),
                vec![Action::ImportRules]
            );
            app.busy = true;
            assert!(click(&mut context, &mut app, submit).is_empty());
            app.busy = false;
            app.rules_imported(Vec::new());
            for _ in 0..4 {
                frame(&mut context, &mut app);
            }
            assert!(click(&mut context, &mut app, [180.0, 40.0]).is_empty());
            assert!(
                app.rule_import_dialog,
                "successful import must release the modal and allow reopening"
            );
            context.io_mut().add_key_event(Key::Escape, true);
            frame(&mut context, &mut app);
            context.io_mut().add_key_event(Key::Escape, false);
            for _ in 0..4 {
                frame(&mut context, &mut app);
            }
            assert!(!app.rule_import_dialog);
            assert!(app.rules.is_empty(), "cancel must not create rules");
            assert!(!app.rules_dirty);
            app.rule_import_source.clear();
        }
    }
}
