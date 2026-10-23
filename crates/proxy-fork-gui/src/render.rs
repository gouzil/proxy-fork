use crate::{
    app::{Action, App},
    ime::ImeBridge,
    logging::LogBuffer,
    preferences::ThemePreference,
};
use anyhow::{Context, Result};
use imgui::{FontConfig, FontGlyphRanges, FontSource, StyleColor, internal::RawCast};
use imgui_wgpu::{Renderer, RendererConfig};
use imgui_winit_support::{HiDpiMode, WinitPlatform};
use std::{
    cell::Cell,
    collections::BTreeSet,
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{
    event::{Event, MouseScrollDelta, WindowEvent},
    event_loop::ActiveEventLoop,
    window::{Theme, Window},
};

thread_local! { static IME_RECT: Cell<([f32; 2], f32)> = const { Cell::new(([0.0,0.0], 20.0)) }; }
unsafe extern "C" fn ime_position(
    _: *mut imgui::sys::ImGuiViewport,
    data: *mut imgui::sys::ImGuiPlatformImeData,
) {
    if !data.is_null() {
        // Dear ImGui invokes this synchronously on the UI thread with a valid ImGuiPlatformImeData.
        let data = unsafe { &*data };
        IME_RECT.with(|r| r.set(([data.InputPos.x, data.InputPos.y], data.InputLineHeight)));
    }
}

struct Clipboard(arboard::Clipboard);
impl imgui::ClipboardBackend for Clipboard {
    fn get(&mut self) -> Option<String> {
        self.0.get_text().ok()
    }
    fn set(&mut self, value: &str) {
        if let Err(error) = self.0.set_text(value) {
            tracing::warn!("Clipboard write failed: {error}");
        }
    }
}

#[derive(PartialEq)]
struct InputState {
    position: [f32; 2],
    buttons: [bool; 5],
    keys: [bool; imgui::Key::COUNT],
}

fn normalize_wheel_delta(delta: MouseScrollDelta, scale: f64) -> MouseScrollDelta {
    match delta {
        MouseScrollDelta::PixelDelta(position) => {
            let position = position.to_logical::<f64>(scale);
            // Match Dear ImGui's native macOS backend without the winit backend's sign-only conversion.
            MouseScrollDelta::LineDelta((position.x * 0.01) as f32, (position.y * 0.01) as f32)
        }
        lines => lines,
    }
}

fn input_needs_redraw(previous: &mut Option<InputState>, ui: &imgui::Ui) -> bool {
    let io = ui.io();
    let state = InputState {
        position: io.mouse_pos,
        buttons: io.mouse_down,
        keys: imgui::Key::VARIANTS.map(|key| ui.is_key_down(key)),
    };
    // ImGui trickles queued input across frames; stop only after processed input settles.
    let changed = previous.as_ref() != Some(&state);
    *previous = Some(state);
    // Read only the live IO size: imgui-rs 0.12's iterator makes a null slice before first text input.
    let has_text = unsafe { io.raw().InputQueueCharacters.Size != 0 };
    changed || io.mouse_wheel != 0.0 || io.mouse_wheel_h != 0.0 || has_text
}

pub fn apply_theme(style: &mut imgui::Style, theme: Theme) {
    let dark = theme == Theme::Dark;
    if dark {
        style.use_dark_colors();
    } else {
        style.use_light_colors();
    }
    for (color, light, dark_color) in [
        (
            StyleColor::Text,
            [0.16, 0.18, 0.22, 1.0],
            [0.90, 0.92, 0.95, 1.0],
        ),
        (
            StyleColor::TextDisabled,
            [0.38, 0.42, 0.48, 1.0],
            [0.67, 0.70, 0.76, 1.0],
        ),
        (
            StyleColor::WindowBg,
            [0.965, 0.971, 0.980, 1.0],
            [0.085, 0.10, 0.125, 1.0],
        ),
        (StyleColor::ChildBg, [1.0; 4], [0.125, 0.14, 0.17, 1.0]),
        (StyleColor::PopupBg, [1.0; 4], [0.125, 0.14, 0.17, 1.0]),
        (
            StyleColor::TitleBg,
            [0.945, 0.952, 0.965, 1.0],
            [0.11, 0.125, 0.155, 1.0],
        ),
        (
            StyleColor::TitleBgActive,
            [0.90, 0.93, 0.98, 1.0],
            [0.18, 0.22, 0.29, 1.0],
        ),
        (
            StyleColor::Border,
            [0.87, 0.89, 0.92, 0.65],
            [0.26, 0.29, 0.34, 0.80],
        ),
        (
            StyleColor::FrameBg,
            [0.94, 0.952, 0.970, 1.0],
            [0.18, 0.20, 0.24, 1.0],
        ),
        (
            StyleColor::FrameBgHovered,
            [0.90, 0.93, 0.98, 1.0],
            [0.23, 0.27, 0.34, 1.0],
        ),
        (
            StyleColor::FrameBgActive,
            [0.86, 0.91, 0.99, 1.0],
            [0.27, 0.32, 0.42, 1.0],
        ),
        (
            StyleColor::Button,
            [0.93, 0.945, 0.970, 1.0],
            [0.20, 0.235, 0.29, 1.0],
        ),
        (
            StyleColor::ButtonHovered,
            [0.86, 0.90, 0.97, 1.0],
            [0.27, 0.32, 0.41, 1.0],
        ),
        (
            StyleColor::ButtonActive,
            [0.80, 0.86, 0.95, 1.0],
            [0.31, 0.38, 0.49, 1.0],
        ),
        (
            StyleColor::Header,
            [0.85, 0.90, 0.99, 1.0],
            [0.16, 0.26, 0.43, 1.0],
        ),
        (
            StyleColor::HeaderHovered,
            [0.82, 0.88, 0.98, 1.0],
            [0.22, 0.33, 0.51, 1.0],
        ),
        (
            StyleColor::HeaderActive,
            [0.76, 0.84, 0.97, 1.0],
            [0.27, 0.39, 0.58, 1.0],
        ),
        (
            StyleColor::CheckMark,
            [0.10, 0.34, 0.75, 1.0],
            [0.60, 0.78, 1.0, 1.0],
        ),
        (
            StyleColor::NavHighlight,
            [0.16, 0.43, 0.94, 1.0],
            [0.60, 0.78, 1.0, 1.0],
        ),
        (
            StyleColor::Separator,
            [0.88, 0.90, 0.93, 1.0],
            [0.26, 0.29, 0.34, 1.0],
        ),
        (StyleColor::ScrollbarBg, [0.0; 4], [0.0; 4]),
        (
            StyleColor::ScrollbarGrab,
            [0.72, 0.76, 0.82, 0.65],
            [0.44, 0.49, 0.57, 0.75],
        ),
    ] {
        style.colors[color as usize] = if dark { dark_color } else { light };
    }
}

pub struct Graphics {
    pub window: Arc<Window>,
    context: imgui::Context,
    theme: ThemePreference,
    effective_theme: Theme,
    glyph_ranges: Box<[u32]>,
    characters: BTreeSet<char>,
    font_dirty: bool,
    settle_frame: bool,
    input_state: Option<InputState>,
    platform: WinitPlatform,
    renderer: Renderer,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    last_frame: Instant,
    pub ime: ImeBridge,
    pub frames: u64,
    pub font_bytes: usize,
    pub text_active: bool,
}
impl Graphics {
    pub async fn new(
        event_loop: &ActiveEventLoop,
        size: [f32; 2],
        theme: ThemePreference,
    ) -> Result<Self> {
        let window = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title("Proxy Fork")
                    .with_theme(theme.native_theme())
                    .with_inner_size(winit::dpi::LogicalSize::new(size[0], size[1]))
                    .with_min_inner_size(winit::dpi::LogicalSize::new(640.0, 480.0)),
            )?,
        );
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: if cfg!(target_os = "macos") {
                wgpu::Backends::METAL
            } else {
                wgpu::Backends::PRIMARY
            },
            ..wgpu::InstanceDescriptor::new_with_display_handle(Box::new(
                event_loop.owned_display_handle(),
            ))
        });
        let surface = instance.create_surface(window.clone())?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await?;
        let size = window.inner_size();
        let config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .context("no compatible surface configuration")?;
        surface.configure(&device, &config);
        let mut context = imgui::Context::create();
        context.set_ini_filename(None);
        context.io_mut().config_flags |= imgui::ConfigFlags::NAV_ENABLE_KEYBOARD;
        context.io_mut().set_platform_ime_data_fn = Some(ime_position);
        if let Ok(clipboard) = arboard::Clipboard::new() {
            context.set_clipboard_backend(Clipboard(clipboard));
        }
        let mut platform = WinitPlatform::new(&mut context);
        platform.attach_window(context.io_mut(), &window, HiDpiMode::Default);
        let style = context.style_mut();
        let effective_theme = theme.resolve(window.theme());
        apply_theme(style, effective_theme);
        style.window_rounding = 0.0;
        style.child_rounding = 10.0;
        style.frame_rounding = 6.0;
        style.popup_rounding = 8.0;
        style.window_padding = [20.0, 20.0];
        style.frame_padding = [10.0, 6.0];
        style.item_spacing = [10.0, 10.0];
        style.scrollbar_size = 10.0;
        style.scrollbar_rounding = 6.0;
        let characters: BTreeSet<char> = (0x20..=0xff)
            .filter_map(char::from_u32)
            .chain(include_str!("../locales/app.yml").chars())
            .filter(|c| !c.is_control())
            .collect();
        let glyph_ranges = Self::ranges(&characters);
        let font_bytes = Self::fonts(&mut context, window.scale_factor(), &glyph_ranges);
        let renderer = Renderer::new(
            &mut context,
            &device,
            &queue,
            RendererConfig {
                texture_format: config.format,
                ..RendererConfig::default()
            },
        );
        Ok(Self {
            window,
            context,
            theme,
            effective_theme,
            glyph_ranges,
            characters,
            font_dirty: false,
            settle_frame: false,
            input_state: None,
            platform,
            renderer,
            device,
            queue,
            surface,
            config,
            last_frame: Instant::now(),
            ime: ImeBridge::default(),
            frames: 0,
            font_bytes,
            text_active: false,
        })
    }
    fn ranges(characters: &BTreeSet<char>) -> Box<[u32]> {
        let mut ranges = Vec::new();
        for ch in characters {
            let code = *ch as u32;
            if ranges.last().is_some_and(|end| *end + 1 == code) {
                *ranges.last_mut().unwrap() = code;
            } else {
                ranges.extend([code, code]);
            }
        }
        ranges.push(0);
        ranges.into_boxed_slice()
    }
    fn fonts(context: &mut imgui::Context, scale: f64, ranges: &[u32]) -> usize {
        context.fonts().clear();
        // Graphics owns this allocation, clears all fonts before replacing it, and drops Context first.
        let ranges: &'static [u32] =
            unsafe { std::slice::from_raw_parts(ranges.as_ptr(), ranges.len()) };
        context.io_mut().font_global_scale = 1.0 / scale as f32;
        context.fonts().add_font(&[FontSource::TtfData {
            data: include_bytes!("../assets/fonts/NotoSansSC.ttf"),
            size_pixels: 18.0 * scale as f32,
            config: Some(FontConfig {
                oversample_h: 1,
                oversample_v: 1,
                glyph_ranges: FontGlyphRanges::from_slice(ranges),
                ..FontConfig::default()
            }),
        }]);
        context.fonts().build_rgba32_texture().data.len()
    }
    fn remember(&mut self, text: &str) {
        for ch in text.chars().filter(|c| !c.is_control()) {
            self.font_dirty |= self.characters.insert(ch);
        }
    }
    fn remember_app(&mut self, app: &App) {
        for text in [
            &app.cert_path,
            &app.key_path,
            &app.update_notes,
            &app.config.listen.host,
        ] {
            self.remember(text);
        }
        if let Some(error) = &app.error {
            self.remember(error);
        }
        for draft in &app.rules {
            for text in [
                &draft.rule.protocol,
                &draft.rule.host,
                &draft.rule.target_host,
            ] {
                self.remember(text);
            }
            for text in [&draft.rule.path, &draft.rule.target_path]
                .into_iter()
                .flatten()
            {
                self.remember(text);
            }
        }
    }
    fn rebuild_fonts(&mut self) {
        self.context.fonts().clear();
        self.glyph_ranges = Self::ranges(&self.characters);
        self.font_bytes = Self::fonts(
            &mut self.context,
            self.window.scale_factor(),
            &self.glyph_ranges,
        );
        self.renderer
            .reload_font_texture(&mut self.context, &self.device, &self.queue);
        self.font_dirty = false;
    }
    pub fn event(&mut self, mut event: WindowEvent) {
        self.settle_frame |= !matches!(event, WindowEvent::RedrawRequested);
        if let WindowEvent::ThemeChanged(theme) = event {
            self.apply_system_theme(Some(theme));
        }
        if let WindowEvent::Ime(ime) = &event {
            match ime {
                winit::event::Ime::Preedit(text, _) | winit::event::Ime::Commit(text) => {
                    self.remember(text)
                }
                _ => {}
            }
            if let Some(text) = self.ime.event(ime) {
                for ch in text.chars() {
                    self.context.io_mut().add_input_character(ch);
                }
            }
        }
        if let WindowEvent::KeyboardInput {
            event: ref mut key, ..
        } = event
        {
            if let Some(text) = &key.text {
                self.remember(text);
            }
            if self.ime.suppress_keyboard_text(key.text.as_deref()) {
                key.text = None;
            }
        }
        if matches!(event, WindowEvent::Focused(false)) {
            self.ime.reset();
        }
        if let WindowEvent::MouseWheel { delta, .. } = &mut event {
            *delta = normalize_wheel_delta(*delta, self.platform.hidpi_factor());
        }
        self.platform.handle_event::<()>(
            self.context.io_mut(),
            &self.window,
            &Event::WindowEvent {
                window_id: self.window.id(),
                event,
            },
        );
    }
    pub fn resize(&mut self) {
        let size = self.window.inner_size();
        if size.width > 0 && size.height > 0 {
            self.config.width = size.width;
            self.config.height = size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }
    pub fn dpi_changed(&mut self) {
        self.rebuild_fonts();
        self.resize();
    }
    pub fn set_theme(&mut self, theme: ThemePreference) {
        self.theme = theme;
        self.window.set_theme(theme.native_theme());
        self.apply_system_theme(self.window.theme());
        self.window.request_redraw();
    }
    fn apply_system_theme(&mut self, system: Option<Theme>) {
        self.effective_theme = self.theme.resolve(system);
        apply_theme(self.context.style_mut(), self.effective_theme);
    }
    pub fn draw(&mut self, app: &mut App, logs: &LogBuffer) -> Result<Vec<Action>> {
        self.remember_app(app);
        if self.font_dirty {
            self.rebuild_fonts();
        }
        let size = self.window.inner_size();
        if size.width == 0 || size.height == 0 {
            return Ok(Vec::new());
        }
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(Vec::new());
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.resize();
                self.window.request_redraw();
                return Ok(Vec::new());
            }
            other => anyhow::bail!("GPU surface failed: {other:?}"),
        };
        let now = Instant::now();
        self.context.io_mut().update_delta_time(
            now.duration_since(self.last_frame)
                .min(Duration::from_secs(1)),
        );
        self.last_frame = now;
        self.platform
            .prepare_frame(self.context.io_mut(), &self.window)?;
        let ui = self.context.frame();
        let actions = crate::view::draw(ui, app, logs, self.effective_theme);
        let input_changed = input_needs_redraw(&mut self.input_state, ui);
        let (position, height) = IME_RECT.with(Cell::get);
        if !self.ime.preedit.is_empty() {
            ui.get_foreground_draw_list().add_text(
                [position[0], position[1] + height],
                [1.0, 0.8, 0.3, 1.0],
                &self.ime.preedit,
            );
        }
        self.text_active = ui.io().want_text_input || ui.io().mouse_down.iter().any(|b| *b);
        self.window.set_ime_allowed(ui.io().want_text_input);
        self.window.set_ime_cursor_area(
            winit::dpi::LogicalPosition::new(position[0], position[1]),
            winit::dpi::LogicalSize::new(2.0, height.max(1.0)),
        );
        self.platform.prepare_render(ui, &self.window);
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Proxy Fork UI"),
            });
        let background = self.context.style().colors[StyleColor::WindowBg as usize];
        let attachments = [Some(wgpu::RenderPassColorAttachment {
            view: &view,
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: background[0] as f64,
                    g: background[1] as f64,
                    b: background[2] as f64,
                    a: 1.0,
                }),
                store: wgpu::StoreOp::Store,
            },
        })];
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Proxy Fork UI"),
            color_attachments: &attachments,
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        self.renderer
            .render(self.context.render(), &self.queue, &self.device, &mut pass)?;
        drop(pass);
        self.queue.submit(Some(encoder.finish()));
        frame.present();
        self.frames += 1;
        self.remember_app(app);
        let settle_frame = std::mem::take(&mut self.settle_frame);
        if self.font_dirty || settle_frame || input_changed {
            self.window.request_redraw();
        }
        Ok(actions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{app::Page, preferences::Preferences};
    use imgui::{Context, MouseButton};
    use proxy_fork_core::app_config::AppConfig;

    #[test]
    fn pixel_wheel_deltas_preserve_distance_direction_and_dpi() {
        use winit::{dpi::PhysicalPosition, event::MouseScrollDelta};

        for (position, scale, expected) in [
            ([1.0, -4.0], 1.0, [0.01, -0.04]),
            ([2.0, -8.0], 2.0, [0.01, -0.04]),
            ([1.5, -6.0], 1.5, [0.01, -0.04]),
            ([20.0, -80.0], 2.0, [0.1, -0.4]),
            ([0.2, -0.8], 2.0, [0.001, -0.004]),
            ([-0.2, 0.8], 2.0, [-0.001, 0.004]),
            ([0.0, 0.0], 2.0, [0.0, 0.0]),
        ] {
            let delta =
                MouseScrollDelta::PixelDelta(PhysicalPosition::new(position[0], position[1]));
            let MouseScrollDelta::LineDelta(x, y) = normalize_wheel_delta(delta, scale) else {
                panic!("pixel scrolling must bypass the backend's sign-only conversion");
            };
            assert!((x - expected[0]).abs() < 1e-6, "horizontal: {position:?}");
            assert!((y - expected[1]).abs() < 1e-6, "vertical: {position:?}");
        }
    }

    #[test]
    fn line_wheel_deltas_keep_the_mouse_wheel_step() {
        use winit::event::MouseScrollDelta;

        for scale in [1.0, 1.5, 2.0] {
            for (x, y) in [(0.0, -1.0), (0.25, 2.5), (-1.0, 0.0), (0.0, 0.0)] {
                assert_eq!(
                    normalize_wheel_delta(MouseScrollDelta::LineDelta(x, y), scale),
                    MouseScrollDelta::LineDelta(x, y),
                );
            }
        }
    }

    #[test]
    fn queued_input_drains_then_rendering_returns_to_idle() {
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
        let logs = LogBuffer::default();
        let mut previous = None;
        let inspections = std::cell::Cell::new(0);
        let frame = |context: &mut Context, app: &mut App, previous: &mut Option<InputState>| {
            let ui = context.frame();
            let actions = crate::view::draw(ui, app, &logs, Theme::Light);
            if !actions.is_empty() {
                assert_eq!(app.page, Page::Settings);
                assert_eq!(actions, vec![crate::app::Action::InspectAutostart]);
                inspections.set(inspections.get() + 1);
            }
            let redraw = input_needs_redraw(previous, ui);
            context.render();
            redraw
        };
        for _ in 0..4 {
            frame(&mut context, &mut app, &mut previous);
        }
        for position in [[80.0, 142.0], [80.0, 192.0], [80.0, 242.0]] {
            context.io_mut().add_mouse_pos_event(position);
            context
                .io_mut()
                .add_mouse_button_event(MouseButton::Left, true);
            context
                .io_mut()
                .add_mouse_button_event(MouseButton::Left, false);
        }
        let mut frames = 0;
        while frame(&mut context, &mut app, &mut previous) {
            frames += 1;
            assert!(
                frames < 32,
                "finite input must not cause continuous rendering"
            );
        }
        assert!(
            frames > 2,
            "the regression must require more than two frames"
        );
        assert_eq!(app.page, Page::Settings, "all queued clicks should settle");
        assert!(!frame(&mut context, &mut app, &mut previous));
        let expected_inspections = usize::from(cfg!(target_os = "macos"));
        assert_eq!(inspections.get(), expected_inspections);

        for _ in 0..3 {
            context.io_mut().add_key_event(imgui::Key::A, true);
            context.io_mut().add_key_event(imgui::Key::A, false);
        }
        let mut presses = 0;
        for index in 0..32 {
            let ui = context.frame();
            presses += usize::from(ui.is_key_pressed(imgui::Key::A));
            let redraw = input_needs_redraw(&mut previous, ui);
            context.render();
            if !redraw {
                break;
            }
            assert!(index < 31, "key input must return to idle");
        }
        assert_eq!(presses, 3);

        context.io_mut().add_mouse_wheel_event([0.0, -1.0]);
        assert!(frame(&mut context, &mut app, &mut previous));
        for index in 0..32 {
            if !frame(&mut context, &mut app, &mut previous) {
                break;
            }
            assert!(index < 31, "wheel input must return to idle");
        }
        context.io_mut().add_input_character('x');
        assert!(frame(&mut context, &mut app, &mut previous));
        assert!(!frame(&mut context, &mut app, &mut previous));
        assert_eq!(inspections.get(), expected_inspections * 2);

        context.io_mut().add_mouse_pos_event([100.0, 100.0]);
        let scroll_frame = |context: &mut Context, reset, previous: &mut Option<InputState>| {
            let ui = context.frame();
            let scroll = ui
                .window("pixel wheel regression")
                .position([0.0, 0.0], imgui::Condition::Always)
                .size([300.0, 300.0], imgui::Condition::Always)
                .build(|| {
                    ui.dummy([100.0, 2000.0]);
                    if reset {
                        ui.set_scroll_y(100.0);
                    }
                    ui.scroll_y()
                })
                .unwrap();
            let redraw = input_needs_redraw(previous, ui);
            context.render();
            (scroll, redraw)
        };
        for index in 0..4 {
            scroll_frame(&mut context, index == 0, &mut previous);
        }
        let (before, _) = scroll_frame(&mut context, false, &mut previous);
        let delta = MouseScrollDelta::PixelDelta(winit::dpi::PhysicalPosition::new(0.0, -20.0));
        let MouseScrollDelta::LineDelta(x, y) = normalize_wheel_delta(delta, 2.0) else {
            panic!("pixel scrolling must be normalized before backend handling");
        };
        context.io_mut().add_mouse_wheel_event([x, y]);
        let (after, redraw) = scroll_frame(&mut context, false, &mut previous);
        assert!(
            redraw,
            "fractional wheel input must schedule a settle frame"
        );
        assert!(after > before, "small deltas must still scroll content");
        assert!(
            after - before < 10.0,
            "small deltas must not jump a whole wheel step"
        );
        for index in 0..8 {
            if !scroll_frame(&mut context, false, &mut previous).1 {
                break;
            }
            assert!(index < 7, "fractional wheel input must return to idle");
        }
    }
}
