use imgui::{Condition, Context, FontSource, MouseButton};
use proxy_fork_core::app_config::AppConfig;
use proxy_fork_gui::{app::App, logging::LogBuffer, preferences::Preferences, view::logs};

fn frame(context: &mut Context, app: &mut App, log: &LogBuffer) {
    let ui = context.frame();
    ui.window("log-test")
        .position([0.0, 0.0], Condition::Always)
        .size(ui.io().display_size, Condition::Always)
        .title_bar(false)
        .build(|| {
            logs::toolbar(ui, app, log, 10.0);
            ui.set_cursor_pos([10.0, 60.0]);
            logs::draw(ui, app, log);
        });
    context.render();
}

fn click(context: &mut Context, app: &mut App, log: &LogBuffer, position: [f32; 2]) {
    context.io_mut().add_mouse_pos_event(position);
    context
        .io_mut()
        .add_mouse_button_event(MouseButton::Left, true);
    context
        .io_mut()
        .add_mouse_button_event(MouseButton::Left, false);
    for _ in 0..4 {
        frame(context, app, log);
    }
}

#[test]
fn log_list_filters_pauses_sorts_clears_and_keeps_full_record_details() {
    let mut context = Context::create();
    context.set_ini_filename(None);
    context.io_mut().display_size = [640.0, 480.0];
    context.io_mut().delta_time = 1.0 / 60.0;
    context
        .fonts()
        .add_font(&[FontSource::DefaultFontData { config: None }]);
    context.fonts().build_rgba32_texture();
    let mut app = App::new(AppConfig::default(), Preferences::default());
    app.set_locale("en");
    let log = LogBuffer::default();
    log.push(tracing::Level::DEBUG, "alpha debug".into());
    log.push(tracing::Level::WARN, "beta warning ###literal".into());
    log.push(tracing::Level::ERROR, "gamma failure".into());
    for _ in 0..4 {
        frame(&mut context, &mut app, &log);
    }
    click(&mut context, &mut app, &log, [40.0, 115.0]);
    assert!(app.log_detail.take().unwrap().ends_with("alpha debug"));
    app.log_filter = "BeTa".into();
    for _ in 0..4 {
        frame(&mut context, &mut app, &log);
    }
    click(&mut context, &mut app, &log, [40.0, 115.0]);
    assert!(
        app.log_detail
            .take()
            .unwrap()
            .ends_with("beta warning ###literal")
    );

    app.log_filter.clear();
    app.log_level = 4; // WARN and ERROR, preserving minimum-level semantics.
    for _ in 0..4 {
        frame(&mut context, &mut app, &log);
    }
    click(&mut context, &mut app, &log, [40.0, 115.0]);
    assert!(
        app.log_detail
            .take()
            .unwrap()
            .ends_with("beta warning ###literal")
    );
    click(&mut context, &mut app, &log, [510.0, 24.0]);
    assert!(app.log_newest_first);
    click(&mut context, &mut app, &log, [40.0, 115.0]);
    assert!(app.log_detail.take().unwrap().ends_with("gamma failure"));

    click(&mut context, &mut app, &log, [408.0, 24.0]);
    assert_eq!(app.paused_logs.as_ref().unwrap().len(), 3);
    log.push(tracing::Level::ERROR, "delta while paused".into());
    for _ in 0..4 {
        frame(&mut context, &mut app, &log);
    }
    click(&mut context, &mut app, &log, [40.0, 115.0]);
    assert!(app.log_detail.take().unwrap().ends_with("gamma failure"));
    click(&mut context, &mut app, &log, [408.0, 24.0]);
    assert!(app.paused_logs.is_none());
    click(&mut context, &mut app, &log, [40.0, 115.0]);
    assert!(
        app.log_detail
            .take()
            .unwrap()
            .ends_with("delta while paused")
    );

    click(&mut context, &mut app, &log, [604.0, 24.0]);
    assert!(log.records.lock().unwrap().is_empty());
    assert!(app.log_detail.is_none());
    app.log_filter = "not found".into();
    for _ in 0..4 {
        frame(&mut context, &mut app, &log);
    }
    click(&mut context, &mut app, &log, [40.0, 115.0]);
    assert!(app.log_detail.is_none());

    app.log_filter.clear();
    app.log_level = 0;
    app.log_newest_first = false;
    app.auto_scroll = true;
    for i in 0..2500 {
        log.push(tracing::Level::INFO, format!("entry {i}"));
    }
    for _ in 0..4 {
        frame(&mut context, &mut app, &log);
    }
    context.io_mut().add_mouse_pos_event([300.0, 200.0]);
    context.io_mut().add_mouse_wheel_event([0.0, 8.0]);
    for _ in 0..20 {
        frame(&mut context, &mut app, &log);
    }
    click(&mut context, &mut app, &log, [40.0, 115.0]);
    assert!(!app.log_detail.take().unwrap().ends_with("entry 2499"));
    log.push(
        tracing::Level::INFO,
        "new arrival must not pull the reader to the bottom".into(),
    );
    for _ in 0..4 {
        frame(&mut context, &mut app, &log);
    }
    click(&mut context, &mut app, &log, [40.0, 115.0]);
    assert!(!app.log_detail.take().unwrap().contains("new arrival"));
    assert_eq!(log.records.lock().unwrap().len(), 2000);
}
