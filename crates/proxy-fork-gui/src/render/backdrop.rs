use anyhow::Result;
use imgui::{DrawCmd, DrawData, internal::RawCast, sys};
use imgui_wgpu::{RenderData, Renderer};

unsafe extern "C" fn modal_boundary(_: *const sys::ImDrawList, _: *const sys::ImDrawCmd) {}

pub(crate) fn mark_modal() {
    // No closure/data allocation: imgui-wgpu ignores callbacks; we use this only as a layer boundary.
    unsafe {
        sys::ImDrawList_AddCallback(
            sys::igGetWindowDrawList(),
            Some(modal_boundary),
            std::ptr::null_mut(),
        );
    }
}

fn layers(data: &DrawData) -> Option<(sys::ImDrawData, sys::ImDrawData)> {
    let split = data.draw_lists().position(|list| {
        list.commands().any(|command| {
            matches!(command, DrawCmd::RawCallback { callback, .. }
                if std::ptr::fn_addr_eq(callback, modal_boundary as unsafe extern "C" fn(_, _)))
        })
    })?;
    // Borrow the original lists without copying vertices; both headers stay inside this render call.
    let mut background = unsafe { *data.raw() };
    let mut foreground = background;
    background.CmdListsCount = split as i32;
    foreground.CmdListsCount -= split as i32;
    foreground.CmdLists = unsafe { foreground.CmdLists.add(split) };
    background.TotalIdxCount = 0;
    background.TotalVtxCount = 0;
    for list in data.draw_lists().take(split) {
        background.TotalIdxCount += list.idx_buffer().len() as i32;
        background.TotalVtxCount += list.vtx_buffer().len() as i32;
    }
    foreground.TotalIdxCount -= background.TotalIdxCount;
    foreground.TotalVtxCount -= background.TotalVtxCount;
    Some((background, foreground))
}

struct Targets {
    size: [u32; 2],
    scene: wgpu::TextureView,
    scratch: wgpu::TextureView,
    sources: [wgpu::BindGroup; 2],
}

pub(super) struct Backdrop {
    format: wgpu::TextureFormat,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    step: wgpu::Buffer,
    horizontal: wgpu::RenderPipeline,
    vertical: wgpu::RenderPipeline,
    targets: Option<Targets>,
    framebuffer: [u32; 2],
    scene_data: Option<RenderData>,
    modal_data: Option<RenderData>,
}

impl Backdrop {
    pub(super) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Modal backdrop"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Modal backdrop"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Modal backdrop"),
            source: wgpu::ShaderSource::Wgsl(include_str!("backdrop.wgsl").into()),
        });
        let pipeline = |entry| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vertex"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        Self {
            format,
            horizontal: pipeline("horizontal"),
            vertical: pipeline("vertical"),
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            step: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Modal blur radius"),
                size: 16,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            layout,
            targets: None,
            framebuffer: [0; 2],
            scene_data: None,
            modal_data: None,
        }
    }

    fn targets(&self, device: &wgpu::Device, size: [u32; 2]) -> Targets {
        let view = || {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("Modal backdrop"),
                    size: wgpu::Extent3d {
                        width: size[0],
                        height: size[1],
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: self.format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor::default())
        };
        let scene = view();
        let scratch = view();
        let sources = [&scene, &scratch].map(|view| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Modal backdrop"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.step.as_entire_binding(),
                    },
                ],
            })
        });
        Targets {
            size,
            scene,
            scratch,
            sources,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn render(
        &mut self,
        data: &DrawData,
        renderer: &mut Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        clear: wgpu::Color,
    ) -> Result<()> {
        let framebuffer = [0, 1]
            .map(|axis| (data.display_size[axis] * data.framebuffer_scale[axis]).ceil() as u32);
        if self.framebuffer != framebuffer {
            self.scene_data = None;
            self.modal_data = None;
            self.framebuffer = framebuffer;
        }
        let Some((mut scene, modal)) = layers(data) else {
            if self.targets.take().is_some() {
                self.scene_data = None;
            }
            self.modal_data = None;
            self.scene_data = Some(renderer.prepare(data, self.scene_data.take(), queue, device));
            return Ok(renderer.split_render(
                data,
                self.scene_data.as_ref().unwrap(),
                &mut pass(encoder, target, wgpu::LoadOp::Clear(clear)),
            )?);
        };
        let size = framebuffer.map(|axis| axis.div_ceil(2).max(1));
        if self.targets.as_ref().is_none_or(|t| t.size != size) {
            self.targets = Some(self.targets(device, size));
            self.scene_data = None;
            self.modal_data = None;
        }
        let targets = self.targets.as_ref().unwrap();
        let step: Vec<_> = [
            1.0 / data.display_size[0],
            1.0 / data.display_size[1],
            0.0,
            0.0,
        ]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect();
        queue.write_buffer(&self.step, 0, &step);
        // imgui-wgpu rounds scissors up; prevent division rounding past the half-size attachment.
        scene.FramebufferScale = [
            (size[0] as f32 / data.display_size[0]).next_down(),
            (size[1] as f32 / data.display_size[1]).next_down(),
        ]
        .into();
        // Raw headers borrow live ImGui lists until the next Context::frame; nothing escapes this call.
        let (scene, modal) = unsafe { (DrawData::from_raw(&scene), DrawData::from_raw(&modal)) };
        self.scene_data = Some(renderer.prepare(scene, self.scene_data.take(), queue, device));
        self.modal_data = Some(renderer.prepare(modal, self.modal_data.take(), queue, device));
        renderer.split_render(
            scene,
            self.scene_data.as_ref().unwrap(),
            &mut pass(encoder, &targets.scene, wgpu::LoadOp::Clear(clear)),
        )?;
        for (pipeline, source, target) in [
            (&self.horizontal, &targets.sources[0], &targets.scratch),
            (&self.vertical, &targets.sources[1], target),
        ] {
            let mut pass = pass(encoder, target, wgpu::LoadOp::Clear(clear));
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, source, &[]);
            pass.draw(0..3, 0..1);
        }
        renderer.split_render(
            modal,
            self.modal_data.as_ref().unwrap(),
            &mut pass(encoder, target, wgpu::LoadOp::Load),
        )?;
        Ok(())
    }
}

fn pass<'a>(
    encoder: &'a mut wgpu::CommandEncoder,
    target: &wgpu::TextureView,
    load: wgpu::LoadOp<wgpu::Color>,
) -> wgpu::RenderPass<'a> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("Proxy Fork UI"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load,
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}
