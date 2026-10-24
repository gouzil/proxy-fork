use super::{apply_theme, backdrop::Backdrop};
use crate::{app::App, preferences::Preferences, view::logs};
use imgui::{Condition, Context, FontSource};
use proxy_fork_core::app_config::AppConfig;
use winit::window::Theme;

#[test]
#[ignore = "requires a real GPU; run explicitly for rendering changes"]
fn modal_blurs_background_on_first_visible_frame_keeps_foreground_sharp_and_restores_scene() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = runtime
        .block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        .unwrap();
    let (device, queue) = runtime
        .block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
        .unwrap();
    eprintln!("Backdrop GPU: {:?}", adapter.get_info());
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let mut context = Context::create();
    context.set_ini_filename(None);
    context
        .fonts()
        .add_font(&[FontSource::DefaultFontData { config: None }]);
    let mut renderer = imgui_wgpu::Renderer::new(
        &mut context,
        &device,
        &queue,
        imgui_wgpu::RendererConfig {
            texture_format: format,
            ..Default::default()
        },
    );
    let mut backdrop = Backdrop::new(&device, format);
    let mut app = App::new(AppConfig::default(), Preferences::default());
    app.set_locale("en");

    let mut render = |viewport: [f32; 2], scale: f32, theme: Theme, modal: bool, delta_time| {
        context.io_mut().display_size = viewport;
        context.io_mut().display_framebuffer_scale = [scale; 2];
        context.io_mut().delta_time = delta_time;
        apply_theme(context.style_mut(), theme);
        app.log_detail = modal.then(|| "12:34:56 INFO sharp foreground".into());
        let ui = context.frame();
        ui.window("Background")
            .position([0.0, 0.0], Condition::Always)
            .size(viewport, Condition::Always)
            .title_bar(false)
            .build(|| {
                let draw = ui.get_window_draw_list();
                for y in 0..32 {
                    for x in 0..32 {
                        let color = if (x + y) % 2 == 0 {
                            [0.0, 0.0, 0.0, 1.0]
                        } else {
                            [1.0; 4]
                        };
                        draw.add_rect(
                            [x as f32 * 4.0, y as f32 * 4.0],
                            [(x + 1) as f32 * 4.0, (y + 1) as f32 * 4.0],
                            color,
                        )
                        .filled(true)
                        .build();
                    }
                }
                draw.add_rect([144.0, 16.0], [224.0, 112.0], [1.0; 4])
                    .filled(true)
                    .build();
                draw.add_rect([183.0, 16.0], [185.0, 112.0], [0.0, 0.0, 0.0, 1.0])
                    .filled(true)
                    .build();
            });
        logs::details(ui, &mut app);
        let data = context.render();
        let visible = data.draw_lists().any(|list| {
            list.commands()
                .any(|command| matches!(command, imgui::DrawCmd::RawCallback { .. }))
        });
        let [width, height] = viewport.map(|d| (d * scale).ceil() as u32);
        let output = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Backdrop test output"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let target = output.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&Default::default());
        backdrop
            .render(
                data,
                &mut renderer,
                &device,
                &queue,
                &mut encoder,
                &target,
                wgpu::Color::BLACK,
            )
            .unwrap();
        let row = (width * 4).div_ceil(256) * 256;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Backdrop readback"),
            size: u64::from(row) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            output.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(height),
                },
            },
            output.size(),
        );
        queue.submit([encoder.finish()]);
        let (sender, receiver) = std::sync::mpsc::channel();
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                sender.send(result).unwrap()
            });
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(std::time::Duration::from_secs(10)),
            })
            .unwrap();
        receiver.recv().unwrap().unwrap();
        let pixels = readback.slice(..).get_mapped_range().to_vec();
        readback.unmap();
        let pixel = |x: f32, y: f32| {
            let offset = ((y * scale) as u32 * row + (x * scale) as u32 * 4) as usize;
            <[u8; 4]>::try_from(&pixels[offset..offset + 4]).unwrap()
        };
        // These are centers of adjacent black/white 4px tiles, outside every modal's bounds.
        let tiles = [pixel(50.0, 50.0), pixel(54.0, 50.0)];
        let foreground = pixel(
            viewport[0] * 0.5 - 300.0 + 16.0,
            viewport[1] * 0.5 - 117.0 + 12.0,
        );
        let stripe: Vec<_> = (0..16).map(|x| pixel(184.0 + x as f32, 64.0)[0]).collect();
        (visible, tiles, foreground, stripe)
    };
    for theme in [Theme::Light, Theme::Dark] {
        for (viewport, scale) in [
            ([940.0, 668.0], 1.0),
            ([640.0, 480.0], 1.0),
            ([640.0, 480.0], 2.0),
            ([941.0, 669.0], 1.5),
        ] {
            let mut last = None;
            for delta in [0.001, 0.02, 0.2, 0.5] {
                let (visible, tiles, foreground, stripe) =
                    render(viewport, scale, theme, true, delta);
                if !visible {
                    continue;
                }
                for tile in tiles {
                    assert!(
                        (70..230).contains(&tile[0]),
                        "must blur actual background, not draw a white veil: {tiles:?}"
                    );
                    assert_eq!(tile[3], 255);
                }
                assert!(
                    tiles[0][0].abs_diff(tiles[1][0]) < 25,
                    "background detail must be blurred: {tiles:?}"
                );
                assert!(
                    stripe
                        .windows(2)
                        .all(|pair| pair[0] <= pair[1].saturating_add(3)),
                    "blur must soften a stripe without repeated ghost edges: {stripe:?}"
                );
                if let Some(previous) = last {
                    assert_eq!(
                        tiles, previous,
                        "effect must not fade in across modal frames"
                    );
                }
                last = Some(tiles);
                let expected = if theme == Theme::Light {
                    [255, 255, 255, 255]
                } else {
                    [32, 36, 43, 255]
                };
                assert_eq!(foreground, expected, "modal itself must stay sharp/opaque");
            }
            assert!(last.is_some(), "must exercise a visible modal");
        }
        let (visible, tiles, _, _) = render([640.0, 480.0], 1.0, theme, false, 0.02);
        assert!(!visible);
        assert_eq!(
            tiles,
            [[0, 0, 0, 255], [255; 4]],
            "closing must immediately restore the unblurred scene"
        );
    }
}
