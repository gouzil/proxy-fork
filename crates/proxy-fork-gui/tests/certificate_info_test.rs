use imgui::{ClipboardBackend, Context, FontSource};
use proxy_fork_core::{CertificateInfo, app_config::AppConfig};
use proxy_fork_gui::{
    app::{Action, App},
    logging::LogBuffer,
    preferences::Preferences,
    view,
};
use std::{cell::RefCell, rc::Rc};
use time::OffsetDateTime;

fn details() -> CertificateInfo {
    CertificateInfo {
        subject: "CN=Displayed CA, O=证书实验室".into(),
        issuer: "CN=Fixture Issuer".into(),
        serial_number: "12:34".into(),
        not_before: OffsetDateTime::UNIX_EPOCH,
        not_after: OffsetDateTime::UNIX_EPOCH + time::Duration::days(365),
        fingerprint: "AB".repeat(32),
    }
}

#[test]
fn refreshing_clears_details_before_a_missing_or_unreadable_certificate() {
    let mut app = App::new(AppConfig::default(), Preferences::default());
    app.ca_info = Some(details());
    app.begin(Action::InspectCa).unwrap();
    assert!(
        app.ca_info.is_none(),
        "a failed refresh must not retain old certificate details"
    );
    app.finish(Err("unreadable certificate".into()));
    assert!(app.ca_info.is_none());
    app.ca_info = Some(details());
    app.ca_not_configured(None);
    assert!(app.ca_info.is_none());
}

struct CapturedClipboard(Rc<RefCell<String>>);
impl ClipboardBackend for CapturedClipboard {
    fn get(&mut self) -> Option<String> {
        Some(self.0.borrow().clone())
    }
    fn set(&mut self, text: &str) {
        *self.0.borrow_mut() = text.into();
    }
}

#[test]
fn certificate_dialog_displays_details_and_distinguishes_validity_from_trust() {
    let captured = Rc::new(RefCell::new(String::new()));
    let mut context = Context::create();
    context.set_ini_filename(None);
    context.set_clipboard_backend(CapturedClipboard(captured.clone()));
    context.io_mut().delta_time = 1.0 / 60.0;
    context
        .fonts()
        .add_font(&[FontSource::DefaultFontData { config: None }]);
    context.fonts().build_rgba32_texture();
    let mut app = App::new(AppConfig::default(), Preferences::default());
    app.ca_dialog = true;
    app.ca_trusted = Some(true);
    app.ca_fingerprint = Some(details().fingerprint);
    let logs = LogBuffer::default();
    context.io_mut().display_size = [940.0, 700.0];
    for _ in 0..4 {
        view::draw(
            context.frame(),
            &mut app,
            &logs,
            winit::window::Theme::Light,
        );
        context.render();
    }
    let mut first = details();
    first.subject = "@first-open".into();
    app.ca_info = Some(first);
    for _ in 0..4 {
        let mut visible_markers = 0;
        let ui = context.frame();
        let marker_uv = unsafe {
            let glyph = &*imgui::sys::ImFont_FindGlyph(imgui::sys::igGetFont(), '@' as _);
            [glyph.U0, glyph.V0]
        };
        view::draw(ui, &mut app, &logs, winit::window::Theme::Light);
        for list in context.render().draw_lists() {
            for command in list.commands() {
                if let imgui::DrawCmd::Elements { count, cmd_params } = command {
                    for index in
                        &list.idx_buffer()[cmd_params.idx_offset..cmd_params.idx_offset + count]
                    {
                        let vertex = list.vtx_buffer()[cmd_params.vtx_offset + *index as usize];
                        if vertex.uv == marker_uv {
                            visible_markers += 1;
                            assert!(
                                vertex.pos[0] >= cmd_params.clip_rect[0],
                                "certificate text must not be clipped on first opening"
                            );
                        }
                    }
                }
            }
        }
        assert!(
            visible_markers > 0,
            "loaded details must be visible on their first frame"
        );
    }
    for viewport in [[940.0, 700.0], [640.0, 480.0]] {
        context.io_mut().display_size = viewport;
        for locale in ["en", "zh-CN"] {
            app.set_locale(locale);
            for validity in ["ca.expired", "ca.valid", "ca.not_yet_valid"] {
                let mut info = details();
                let future = OffsetDateTime::from_unix_timestamp(253_402_300_799).unwrap();
                if validity != "ca.expired" {
                    info.not_after = future;
                }
                if validity == "ca.not_yet_valid" {
                    info.not_before = future - time::Duration::days(1);
                }
                app.ca_info = Some(info);
                for _ in 0..4 {
                    assert!(
                        view::draw(
                            context.frame(),
                            &mut app,
                            &logs,
                            winit::window::Theme::Light
                        )
                        .is_empty()
                    );
                    context.render();
                }
                let ui = context.frame();
                unsafe {
                    imgui::sys::igLogToClipboard(-1);
                }
                view::dashboard::certificate_dialog(ui, &mut app, &mut Vec::new());
                unsafe {
                    imgui::sys::igLogFinish();
                }
                context.render();
                let text = captured.borrow();
                for expected in [
                    "CN=Displayed CA, O=证书实验室",
                    "CN=Fixture Issuer",
                    "12:34",
                ] {
                    assert!(
                        text.contains(expected),
                        "certificate details must be rendered: {expected}\n{text}"
                    );
                }
                assert!(
                    text.contains(proxy_fork_gui::_rust_i18n_translate(locale, validity).as_ref())
                );
                assert!(
                    text.contains(
                        proxy_fork_gui::_rust_i18n_translate(locale, "ca.trusted").as_ref()
                    )
                );
                if validity == "ca.expired" {
                    assert!(text.contains("1970-01-01T00:00:00Z"));
                    assert!(text.contains("1971-01-01T00:00:00Z"));
                }
            }
        }
    }
}
