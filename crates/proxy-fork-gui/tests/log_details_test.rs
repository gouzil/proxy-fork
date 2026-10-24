use imgui::{Context, FontSource, Key, MouseButton, StyleColor};
use proxy_fork_core::app_config::AppConfig;
use proxy_fork_gui::{
    app::{App, Page},
    logging::LogBuffer,
    preferences::Preferences,
    render::apply_theme,
    view,
};

fn frame(context: &mut Context, app: &mut App, log: &LogBuffer) -> Option<[f32; 4]> {
    let popup =
        imgui::ImColor32::from(context.style().colors[StyleColor::PopupBg as usize]).to_rgba();
    let actions = view::draw(context.frame(), app, log, winit::window::Theme::Light);
    assert!(actions.is_empty());
    let draw_data = context.render();
    let vertices: Vec<_> = draw_data
        .draw_lists()
        .map(|list| {
            list.vtx_buffer()
                .iter()
                .filter(|vertex| vertex.col == popup)
                .map(|vertex| vertex.pos)
                .collect::<Vec<_>>()
        })
        .filter(|vertices| !vertices.is_empty())
        .last()
        .unwrap_or_default();
    let popup_width = vertices
        .iter()
        .map(|p| p[0])
        .fold(f32::NEG_INFINITY, f32::max)
        - vertices.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min);
    if app.log_detail.is_some() && popup_width > 300.0 {
        assert!(
            draw_data.draw_lists().any(|list| {
                list.commands()
                    .any(|command| matches!(command, imgui::DrawCmd::RawCallback { .. }))
            }),
            "the backdrop must be submitted with every visible modal frame, not fade in later"
        );
    }
    (!vertices.is_empty()).then(|| {
        vertices.iter().fold(
            [
                f32::INFINITY,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
            ],
            |[left, top, right, bottom], &[x, y]| {
                [left.min(x), top.min(y), right.max(x), bottom.max(y)]
            },
        )
    })
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
fn log_details_stay_centered_with_reachable_close_and_block_background_navigation() {
    let mut context = Context::create();
    context.set_ini_filename(None);
    context.io_mut().delta_time = 1.0 / 60.0;
    context.fonts().add_font(&[FontSource::TtfData {
        data: include_bytes!("../assets/fonts/NotoSansSC.ttf"),
        size_pixels: 18.0,
        config: Some(imgui::FontConfig {
            oversample_h: 1,
            oversample_v: 1,
            glyph_ranges: imgui::FontGlyphRanges::chinese_simplified_common(),
            ..Default::default()
        }),
    }]);
    context.fonts().build_rgba32_texture();
    let mut app = App::new(AppConfig::default(), Preferences::default());
    app.page = Page::Logs;
    let log = LogBuffer::default();
    for viewport in [[940.0, 668.0], [640.0, 480.0]] {
        context.io_mut().display_size = viewport;
        for locale in ["en", "zh-CN"] {
            app.set_locale(locale);
            for theme in [winit::window::Theme::Light, winit::window::Theme::Dark] {
                apply_theme(context.style_mut(), theme);
                for message in [
                    "12:34:56 INFO GUI initialized ###literal".to_owned(),
                    format!(
                        "12:34:56 ERROR {} end-of-record",
                        "详细日志 / long-message ".repeat(75)
                    ),
                ] {
                    app.log_detail = Some(message.clone());
                    for _ in 0..4 {
                        frame(&mut context, &mut app, &log);
                    }
                    assert!(
                        unsafe {
                            imgui::sys::igIsPopupOpen(
                                std::ptr::null(),
                                (imgui::sys::ImGuiPopupFlags_AnyPopupId
                                    | imgui::sys::ImGuiPopupFlags_AnyPopupLevel)
                                    as i32,
                            )
                        },
                        "log details must use the shared modal, not a floating debug window"
                    );
                    let [left, top, right, bottom] = frame(&mut context, &mut app, &log).unwrap();
                    assert!(left >= 15.0 && top >= 15.0);
                    assert!(right <= viewport[0] - 15.0 && bottom <= viewport[1] - 15.0);
                    assert!(
                        ((left + right) * 0.5 - viewport[0] * 0.5).abs() < 2.0,
                        "popup bounds {left},{top},{right},{bottom} at {viewport:?}, {locale}, {theme:?}, {} bytes",
                        message.len()
                    );
                    assert!(((top + bottom) * 0.5 - viewport[1] * 0.5).abs() < 2.0);
                    if message.len() < 100 {
                        assert!(
                            bottom - top <= 280.0,
                            "short records must not open a large empty pane"
                        );
                    }
                    click(&mut context, &mut app, &log, [16.0, 242.0]);
                    assert_eq!(
                        app.page,
                        Page::Logs,
                        "a detail modal must block Settings behind it"
                    );
                    context
                        .io_mut()
                        .add_mouse_pos_event([viewport[0] * 0.5, viewport[1] * 0.5]);
                    context.io_mut().add_mouse_wheel_event([0.0, -30.0]);
                    for _ in 0..8 {
                        frame(&mut context, &mut app, &log);
                    }
                    assert_eq!(app.log_detail.as_deref(), Some(message.as_str()));
                    click(&mut context, &mut app, &log, [right - 72.0, bottom - 37.0]);
                    assert!(
                        app.log_detail.is_none(),
                        "Close must remain fixed below long content"
                    );
                    app.log_detail = Some(message);
                    for _ in 0..4 {
                        frame(&mut context, &mut app, &log);
                    }
                    context.io_mut().add_key_event(Key::Escape, true);
                    frame(&mut context, &mut app, &log);
                    context.io_mut().add_key_event(Key::Escape, false);
                    for _ in 0..4 {
                        frame(&mut context, &mut app, &log);
                    }
                    assert!(app.log_detail.is_none(), "Escape must dismiss the details");
                }
            }
        }
    }
}
