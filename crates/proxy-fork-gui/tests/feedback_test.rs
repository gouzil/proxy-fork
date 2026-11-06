use imgui::{Context, FontSource, Key, MouseButton};
use proxy_fork_core::app_config::AppConfig;
use proxy_fork_gui::{
    app::{App, Page},
    logging::LogBuffer,
    preferences::Preferences,
    view,
};

fn frame(context: &mut Context, app: &mut App, log: &LogBuffer) -> bool {
    let ui = context.frame();
    assert!(view::draw(ui, app, log, winit::window::Theme::Light).is_empty());
    let alert = unsafe { imgui::sys::igIsPopupOpen(c"###operation-error".as_ptr(), 0) };
    context.render();
    alert
}

#[test]
fn action_failure_opens_an_alert_instead_of_scrolling_certificate_content() {
    let mut context = Context::create();
    context.set_ini_filename(None);
    context.io_mut().display_size = [940.0, 700.0];
    context.io_mut().delta_time = 1.0 / 60.0;
    context
        .fonts()
        .add_font(&[FontSource::DefaultFontData { config: None }]);
    context.fonts().build_rgba32_texture();
    let mut app = App::new(AppConfig::default(), Preferences::default());
    app.set_locale("en");
    let log = LogBuffer::default();
    for size in [[940.0, 700.0], [640.0, 480.0]] {
        context.io_mut().display_size = size;
        for locale in ["en", "zh-CN"] {
            app.set_locale(locale);
            for long in [false, true] {
                app.ca_dialog = true;
                for _ in 0..4 {
                    frame(&mut context, &mut app, &log);
                }
                let message = if long {
                    "Diagnostic details ".repeat(300)
                } else {
                    proxy_fork_gui::_rust_i18n_translate(locale, "ca.expired_error").into_owned()
                };
                app.finish(Err(message));
                for _ in 0..4 {
                    frame(&mut context, &mut app, &log);
                }
                assert!(
                    frame(&mut context, &mut app, &log),
                    "action errors must appear in a dedicated modal alert"
                );
                context.io_mut().add_mouse_pos_event([80.0, 142.0]);
                context
                    .io_mut()
                    .add_mouse_button_event(MouseButton::Left, true);
                context
                    .io_mut()
                    .add_mouse_button_event(MouseButton::Left, false);
                for _ in 0..4 {
                    frame(&mut context, &mut app, &log);
                }
                assert_eq!(
                    app.page,
                    Page::Dashboard,
                    "alerts must block background navigation"
                );
                context.io_mut().add_key_event(Key::Escape, true);
                frame(&mut context, &mut app, &log);
                context.io_mut().add_key_event(Key::Escape, false);
                for _ in 0..4 {
                    frame(&mut context, &mut app, &log);
                }
                assert!(!frame(&mut context, &mut app, &log));
                assert!(app.error_dialog.is_none());
                assert!(
                    app.ca_dialog,
                    "keep the certificate workflow available after dismissal"
                );
                let ui = context.frame();
                view::draw(ui, &mut app, &log, winit::window::Theme::Light);
                assert!(
                    unsafe { imgui::sys::igIsPopupOpen(c"###ca-dialog".as_ptr(), 0) },
                    "restore the certificate modal without reopening the error"
                );
                context.render();
            }
        }
    }
    for size in [[940.0, 700.0], [640.0, 480.0]] {
        context.io_mut().display_size = size;
        app.finish(Ok("ca.generated".into()));
        let ui = context.frame();
        view::draw(ui, &mut app, &log, winit::window::Theme::Light);
        let data = context.render();
        let top_layer = data.draw_lists().last().expect("visible feedback");
        assert!(
            !top_layer.vtx_buffer().is_empty()
                && top_layer.vtx_buffer().iter().all(|vertex| {
                    vertex.pos[0] >= size[0] - 398.0
                        && vertex.pos[0] <= size[0] - 14.0
                        && vertex.pos[1] >= 14.0
                        && vertex.pos[1] < 160.0
                }),
            "the success toast must render above the modal on its first frame"
        );
    }
}
