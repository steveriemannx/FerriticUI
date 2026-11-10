//! The FerriticUI renderer on top of the `blade-graphics` HAL.
//!
//! v0 draws two kinds of primitives:
//! * solid rectangles (CPU-transformed to NDC, alpha blended),
//! * alpha-masked glyph quads sampled from the glyph atlas.
//!
//! The window path presents through a swap chain; [`render_offscreen`] renders
//! the same scene into a texture and reads it back, which is what the snapshot
//! tests use.

// The GPU HAL is initialized through one `unsafe` call into `blade-graphics`,
// and the snapshot readback maps the shared buffer.
#![deny(unsafe_op_in_unsafe_fn)]
#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use blade_graphics as blade;
use blade_util::{BufferBelt, BufferBeltDescriptor};
use ferritic_core::{AtlasUpdate, Color, Prim, Rect, Scene, Size};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

const SHADER_SOURCE: &str = include_str!("shader.wgsl");
const ATLAS_SIZE: u32 = 1024;

#[derive(blade_macros::ShaderData)]
struct TextureParams {
    r_texture: blade::TextureView,
    r_sampler: blade::Sampler,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable, blade_macros::Vertex)]
struct SolidVertex {
    pos: [f32; 2],
    pixel: [f32; 2],
    center: [f32; 2],
    half_size: [f32; 2],
    radii: [f32; 4],
    fill: [f32; 4],
    border_width: f32,
    border_color: [f32; 4],
    blur: f32,
    mode: f32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable, blade_macros::Vertex)]
struct TextureVertex {
    pos: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

struct Pipelines {
    color: blade::RenderPipeline,
    texture: blade::RenderPipeline,
}

impl Pipelines {
    fn new(context: &blade::Context, format: blade::TextureFormat) -> Self {
        let shader = context.create_shader(blade::ShaderDesc {
            source: SHADER_SOURCE,
            naga_module: None,
        });

        let color_vertex_layout = <SolidVertex as blade::Vertex>::layout();
        let color = context.create_render_pipeline(blade::RenderPipelineDesc {
            name: "ferritic_solid",
            data_layouts: &[],
            vertex: shader.at("vs_color"),
            vertex_fetches: &[blade::VertexFetchState {
                layout: &color_vertex_layout,
                instanced: false,
            }],
            primitive: blade::PrimitiveState {
                topology: blade::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            fragment: Some(shader.at("fs_color")),
            color_targets: &[blade::ColorTargetState {
                format,
                blend: Some(blade::BlendState::ALPHA_BLENDING),
                write_mask: blade::ColorWrites::all(),
            }],
            multisample_state: blade::MultisampleState::default(),
        });

        let texture_layout = <TextureParams as blade::ShaderData>::layout();
        let texture_vertex_layout = <TextureVertex as blade::Vertex>::layout();
        let texture = context.create_render_pipeline(blade::RenderPipelineDesc {
            name: "ferritic_text",
            data_layouts: &[&texture_layout],
            vertex: shader.at("vs_texture"),
            vertex_fetches: &[blade::VertexFetchState {
                layout: &texture_vertex_layout,
                instanced: false,
            }],
            primitive: blade::PrimitiveState {
                topology: blade::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            fragment: Some(shader.at("fs_texture")),
            color_targets: &[blade::ColorTargetState {
                format,
                blend: Some(blade::BlendState::ALPHA_BLENDING),
                write_mask: blade::ColorWrites::all(),
            }],
            multisample_state: blade::MultisampleState::default(),
        });

        Self { color, texture }
    }

    fn destroy(&mut self, context: &blade::Context) {
        context.destroy_render_pipeline(&mut self.color);
        context.destroy_render_pipeline(&mut self.texture);
    }
}

struct AtlasTexture {
    texture: blade::Texture,
    view: blade::TextureView,
    sampler: blade::Sampler,
    initialized: bool,
    version: u64,
}

impl AtlasTexture {
    fn new(context: &blade::Context) -> Self {
        let size = blade::Extent {
            width: ATLAS_SIZE,
            height: ATLAS_SIZE,
            depth: 1,
        };
        let texture = context.create_texture(blade::TextureDesc {
            name: "ferritic_glyph_atlas",
            format: blade::TextureFormat::R8Unorm,
            size,
            array_layer_count: 1,
            mip_level_count: 1,
            dimension: blade::TextureDimension::D2,
            usage: blade::TextureUsage::COPY | blade::TextureUsage::RESOURCE,
            sample_count: 1,
            external: None,
        });
        let view = context.create_texture_view(
            texture,
            blade::TextureViewDesc {
                name: "ferritic_glyph_atlas_view",
                format: blade::TextureFormat::R8Unorm,
                dimension: blade::ViewDimension::D2,
                subresources: &blade::TextureSubresources::default(),
            },
        );
        let sampler = context.create_sampler(blade::SamplerDesc {
            name: "ferritic_glyph_atlas_sampler",
            address_modes: [blade::AddressMode::ClampToEdge; 3],
            mag_filter: blade::FilterMode::Linear,
            min_filter: blade::FilterMode::Linear,
            mipmap_filter: blade::FilterMode::Nearest,
            ..Default::default()
        });
        Self {
            texture,
            view,
            sampler,
            initialized: false,
            version: 0,
        }
    }

    fn destroy(&mut self, context: &blade::Context) {
        context.destroy_sampler(self.sampler);
        context.destroy_texture_view(self.view);
        context.destroy_texture(self.texture);
    }
}

fn new_belt() -> BufferBelt {
    BufferBelt::new(BufferBeltDescriptor {
        memory: blade::Memory::Shared,
        min_chunk_size: 0x4000,
        alignment: 4,
        target: blade::BufferTarget::Data,
    })
}

/// The GPU device and the per-frame command encoder.
pub struct Gpu {
    pub context: blade::Context,
    encoder: blade::CommandEncoder,
    previous_sync: Option<blade::SyncPoint>,
}

impl Gpu {
    pub fn new() -> Result<Self, String> {
        Self::with_presentation(true)
    }

    pub fn new_offscreen() -> Result<Self, String> {
        Self::with_presentation(false)
    }

    fn with_presentation(presentation: bool) -> Result<Self, String> {
        let context = unsafe {
            blade::Context::init(blade::ContextDesc {
                presentation,
                validation: cfg!(debug_assertions),
                ..Default::default()
            })
        }
        .map_err(|error| error.to_string())?;

        let encoder = context.create_command_encoder(blade::CommandEncoderDesc {
            name: "ferritic",
            buffer_count: 2,
            manual_barriers: false,
        });

        Ok(Self {
            context,
            encoder,
            previous_sync: None,
        })
    }

    pub fn wait_idle(&mut self) {
        if let Some(sync) = self.previous_sync.take() {
            let _ = self.context.wait_for(&sync, !0);
        }
    }
}

fn extent(size: Size) -> blade::Extent {
    blade::Extent {
        width: size.width.max(1.0) as u32,
        height: size.height.max(1.0) as u32,
        depth: 1,
    }
}

fn surface_config(size: Size) -> blade::SurfaceConfig {
    blade::SurfaceConfig {
        size: extent(size),
        usage: blade::TextureUsage::TARGET,
        display_sync: blade::DisplaySync::Recent,
        color_space: blade::ColorSpace::Srgb,
        ..Default::default()
    }
}

enum BatchVertices {
    Solid(Vec<SolidVertex>),
    Texture(Vec<TextureVertex>),
}

struct Batch {
    vertices: BatchVertices,
    scissor: blade::ScissorRect,
}

fn make_scissor(clip: Option<Rect>, physical_size: Size, scale: f32) -> blade::ScissorRect {
    let (left, top, right, bottom) = match clip {
        None => (0.0, 0.0, physical_size.width, physical_size.height),
        Some(clip) => (
            (clip.x * scale).floor(),
            (clip.y * scale).floor(),
            (clip.right() * scale).ceil(),
            (clip.bottom() * scale).ceil(),
        ),
    };
    let left = left.max(0.0).min(physical_size.width) as i32;
    let top = top.max(0.0).min(physical_size.height) as i32;
    let right = right.max(0.0).min(physical_size.width) as i32;
    let bottom = bottom.max(0.0).min(physical_size.height) as i32;
    blade::ScissorRect {
        x: left,
        y: top,
        w: (right - left).max(0) as u32,
        h: (bottom - top).max(0) as u32,
    }
}

/// Splits the scene into draw batches: runs of primitives of the same kind
/// (solid / texture) that share the same scissor. Batches preserve paint order,
/// so blending stays correct inside each clip.
fn build_batches(scene: &Scene, physical_size: Size, scale: f32) -> Vec<Batch> {
    let mut batches: Vec<Batch> = Vec::new();
    for prim in &scene.prims {
        let (clip, solid) = match *prim {
            Prim::Rect { clip, .. } | Prim::Shadow { clip, .. } => (clip, true),
            Prim::Glyph { clip, .. } => (clip, false),
        };
        let scissor = make_scissor(clip, physical_size, scale);
        if scissor.w == 0 || scissor.h == 0 {
            continue;
        }
        let same_batch = batches.last().is_some_and(|batch| {
            batch.scissor == scissor
                && matches!(
                    (&batch.vertices, solid),
                    (BatchVertices::Solid(_), true) | (BatchVertices::Texture(_), false)
                )
        });
        if !same_batch {
            let vertices = if solid {
                BatchVertices::Solid(Vec::new())
            } else {
                BatchVertices::Texture(Vec::new())
            };
            batches.push(Batch { vertices, scissor });
        }
        let batch = batches.last_mut().unwrap();
        match (&mut batch.vertices, *prim) {
            (
                BatchVertices::Solid(out),
                Prim::Rect {
                    rect,
                    color,
                    radius,
                    border_width,
                    border_color,
                    ..
                },
            ) => push_solid(
                out,
                rect,
                color,
                radius,
                border_width,
                border_color,
                physical_size,
                scale,
            ),
            (
                BatchVertices::Solid(out),
                Prim::Shadow {
                    rect,
                    radius,
                    color,
                    blur,
                    offset,
                    ..
                },
            ) => push_shadow(
                out,
                rect,
                radius,
                color,
                blur,
                offset,
                physical_size,
                scale,
            ),
            (BatchVertices::Texture(out), Prim::Glyph { rect, uv, color, .. }) => {
                push_texture_quad(out, rect, uv, color, physical_size, scale);
            }
            _ => unreachable!("a batch only receives primitives of its own kind"),
        }
    }
    batches
}

fn draw_batches(
    context: &blade::Context,
    pass: &mut blade::RenderCommandEncoder<'_>,
    belt: &mut BufferBelt,
    pipelines: &Pipelines,
    atlas: &AtlasTexture,
    batches: &[Batch],
) {
    for batch in batches {
        match &batch.vertices {
            BatchVertices::Solid(vertices) => {
                let buffer = belt.alloc_bytes(bytemuck::cast_slice(vertices), context);
                let mut pipeline = pass.with(&pipelines.color);
                pipeline.set_scissor_rect(&batch.scissor);
                pipeline.bind_vertex(0, buffer);
                pipeline.draw(0, vertices.len() as u32, 0, 1);
            }
            BatchVertices::Texture(vertices) => {
                let buffer = belt.alloc_bytes(bytemuck::cast_slice(vertices), context);
                let mut pipeline = pass.with(&pipelines.texture);
                pipeline.set_scissor_rect(&batch.scissor);
                pipeline.bind(
                    0,
                    &TextureParams {
                        r_texture: atlas.view,
                        r_sampler: atlas.sampler,
                    },
                );
                pipeline.bind_vertex(0, buffer);
                pipeline.draw(0, vertices.len() as u32, 0, 1);
            }
        }
    }
}

/// A window's swap chain, pipelines and the glyph atlas.
pub struct WindowSurface {
    surface: blade::Surface,
    format: blade::TextureFormat,
    /// Physical pixels of the swap chain.
    size: Size,
    /// Logical pixels the UI is laid out in.
    logical_size: Size,
    scale: f32,
    pipelines: Pipelines,
    belt: BufferBelt,
    atlas: AtlasTexture,
}

impl WindowSurface {
    pub fn new<W: HasWindowHandle + HasDisplayHandle>(
        gpu: &Gpu,
        window: &W,
        logical_size: Size,
        scale: f32,
    ) -> Result<Self, String> {
        let context = &gpu.context;
        let physical_size = Size::new(logical_size.width * scale, logical_size.height * scale);
        let surface = context
            .create_surface_configured(window, surface_config(physical_size))
            .map_err(|error| error.to_string())?;
        let format = surface.info().format;
        Ok(Self {
            surface,
            format,
            size: physical_size,
            logical_size,
            scale,
            pipelines: Pipelines::new(context, format),
            belt: new_belt(),
            atlas: AtlasTexture::new(context),
        })
    }

    pub fn size(&self) -> Size {
        self.logical_size
    }

    pub fn format(&self) -> blade::TextureFormat {
        self.format
    }

    pub fn resize(&mut self, gpu: &Gpu, logical_size: Size, scale: f32) {
        if logical_size.is_empty() {
            return;
        }
        self.logical_size = logical_size;
        self.scale = scale;
        self.size = Size::new(logical_size.width * scale, logical_size.height * scale);
        gpu.context
            .reconfigure_surface(&mut self.surface, surface_config(self.size));
    }

    pub fn destroy(&mut self, gpu: &mut Gpu) {
        gpu.wait_idle();
        let Gpu { context, encoder, .. } = gpu;
        self.pipelines.destroy(context);
        self.belt.destroy(context);
        self.atlas.destroy(context);
        context.destroy_surface(&mut self.surface);
        context.destroy_command_encoder(encoder);
    }

    pub fn render(
        &mut self,
        gpu: &mut Gpu,
        scene: &Scene,
        atlas: Option<&AtlasUpdate>,
    ) -> Result<(), String> {
        if self.size.is_empty() {
            return Ok(());
        }

        let frame = self.surface.acquire_frame();
        let Gpu {
            context,
            encoder,
            previous_sync,
        } = gpu;

        encoder.start();
        encoder.init_texture(frame.texture());

        if let Some(update) = atlas {
            if update.version != self.atlas.version {
                if !self.atlas.initialized {
                    encoder.init_texture(self.atlas.texture);
                    self.atlas.initialized = true;
                }
                let source = self.belt.alloc_bytes(&update.pixels, context);
                {
                    let mut transfer = encoder.transfer("ferritic_atlas");
                    transfer.copy_buffer_to_texture(
                        source,
                        update.width,
                        blade::TexturePiece {
                            texture: self.atlas.texture,
                            mip_level: 0,
                            array_layer: 0,
                            origin: [0, 0, 0],
                        },
                        blade::Extent {
                            width: update.width,
                            height: update.height,
                            depth: 1,
                        },
                    );
                }
                self.atlas.version = update.version;
            }
        }

        let batches = build_batches(scene, self.size, self.scale);
        {
            let mut pass = encoder.render(
                "ferritic",
                blade::RenderTargetSet {
                    colors: &[blade::RenderTarget {
                        view: frame.texture_view(),
                        init_op: blade::InitOp::Clear(blade::TextureColor::TransparentBlack),
                        finish_op: blade::FinishOp::Store,
                    }],
                    depth_stencil: None,
                },
            );
            draw_batches(
                context,
                &mut pass,
                &mut self.belt,
                &self.pipelines,
                &self.atlas,
                &batches,
            );
        }

        encoder.present(frame);
        let sync = context.submit(encoder);
        if let Some(previous) = previous_sync.take() {
            let _ = context.wait_for(&previous, !0);
        }
        *previous_sync = Some(sync);
        self.belt.flush(previous_sync.as_ref().unwrap());
        self.belt.trim(4, context);
        Ok(())
    }
}

/// A rendered frame, as RGBA8 sRGB pixel data.
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// Renders a scene into an offscreen texture and reads it back.
pub fn render_offscreen(
    gpu: &mut Gpu,
    logical_size: Size,
    scale: f32,
    scene: &Scene,
    atlas: Option<&AtlasUpdate>,
) -> Result<Snapshot, String> {
    let physical = Size::new(logical_size.width * scale, logical_size.height * scale);
    let physical_extent = extent(physical);
    let format = blade::TextureFormat::Rgba8Unorm;

    let Gpu {
        context,
        encoder,
        previous_sync,
    } = gpu;
    let _ = previous_sync.take();

    let texture = context.create_texture(blade::TextureDesc {
        name: "ferritic_snapshot_target",
        format,
        size: physical_extent,
        array_layer_count: 1,
        mip_level_count: 1,
        dimension: blade::TextureDimension::D2,
        usage: blade::TextureUsage::TARGET | blade::TextureUsage::COPY,
        sample_count: 1,
        external: None,
    });
    let view = context.create_texture_view(
        texture,
        blade::TextureViewDesc {
            name: "ferritic_snapshot_view",
            format,
            dimension: blade::ViewDimension::D2,
            subresources: &blade::TextureSubresources::default(),
        },
    );
    let byte_count = (physical_extent.width * physical_extent.height * 4) as u64;
    let readback = context.create_buffer(blade::BufferDesc {
        name: "ferritic_snapshot_readback",
        size: byte_count,
        memory: blade::Memory::Shared,
    });

    let mut pipelines = Pipelines::new(context, format);
    let mut belt = new_belt();
    let mut atlas_texture = AtlasTexture::new(context);

    encoder.start();
    encoder.init_texture(texture);
    encoder.init_texture(atlas_texture.texture);
    atlas_texture.initialized = true;

    if let Some(update) = atlas {
        atlas_texture.version = update.version;
        let source = belt.alloc_bytes(&update.pixels, context);
        {
            let mut transfer = encoder.transfer("ferritic_snapshot_atlas");
            transfer.copy_buffer_to_texture(
                source,
                update.width,
                blade::TexturePiece {
                    texture: atlas_texture.texture,
                    mip_level: 0,
                    array_layer: 0,
                    origin: [0, 0, 0],
                },
                blade::Extent {
                    width: update.width,
                    height: update.height,
                    depth: 1,
                },
            );
        }
    }

    let batches = build_batches(scene, physical, scale);
    {
        let mut pass = encoder.render(
            "ferritic_snapshot",
            blade::RenderTargetSet {
                colors: &[blade::RenderTarget {
                    view,
                    init_op: blade::InitOp::Clear(blade::TextureColor::TransparentBlack),
                    finish_op: blade::FinishOp::Store,
                }],
                depth_stencil: None,
            },
        );
        draw_batches(
            context,
            &mut pass,
            &mut belt,
            &pipelines,
            &atlas_texture,
            &batches,
        );
    }

    {
        let mut transfer = encoder.transfer("ferritic_snapshot_readback");
        transfer.copy_texture_to_buffer(
            texture.into(),
            readback.into(),
            physical_extent.width * 4,
            physical_extent,
        );
    }

    let sync = context.submit(encoder);
    context
        .wait_for(&sync, 120_000)
        .map_err(|error| error.to_string())?;

    let mut pixels = vec![0u8; byte_count as usize];
    unsafe {
        std::ptr::copy_nonoverlapping(readback.data(), pixels.as_mut_ptr(), byte_count as usize);
    }

    belt.destroy(context);
    pipelines.destroy(context);
    atlas_texture.destroy(context);
    context.destroy_buffer(readback);
    context.destroy_texture_view(view);
    context.destroy_texture(texture);

    Ok(Snapshot {
        width: physical_extent.width,
        height: physical_extent.height,
        pixels,
    })
}

fn ndc(point: [f32; 2], size: Size) -> [f32; 2] {
    [
        point[0] / size.width.max(1.0) * 2.0 - 1.0,
        1.0 - point[1] / size.height.max(1.0) * 2.0,
    ]
}

#[allow(clippy::too_many_arguments)]
fn push_solid(
    out: &mut Vec<SolidVertex>,
    rect: Rect,
    color: Color,
    radius: f32,
    border_width: f32,
    border_color: Color,
    size: Size,
    scale: f32,
) {
    let left = rect.x * scale;
    let top = rect.y * scale;
    let right = rect.right() * scale;
    let bottom = rect.bottom() * scale;
    let center = [(left + right) * 0.5, (top + bottom) * 0.5];
    let half_size = [(right - left) * 0.5, (bottom - top) * 0.5];
    let radius = radius * scale;
    let corners = [
        [left, top],
        [right, top],
        [right, bottom],
        [left, bottom],
    ];
    let positions = corners.map(|corner| ndc(corner, size));
    let fill = color.to_array();
    let border_color = border_color.to_array();
    for index in [0u32, 1, 2, 0, 2, 3] {
        let index = index as usize;
        out.push(SolidVertex {
            pos: positions[index],
            pixel: corners[index],
            center,
            half_size,
            radii: [radius; 4],
            fill,
            border_width: border_width * scale,
            border_color,
            blur: 0.0,
            mode: 0.0,
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn push_shadow(
    out: &mut Vec<SolidVertex>,
    rect: Rect,
    radius: f32,
    color: Color,
    blur: f32,
    offset: (f32, f32),
    size: Size,
    scale: f32,
) {
    let left = (rect.x + offset.0 - blur) * scale;
    let top = (rect.y + offset.1 - blur) * scale;
    let right = (rect.right() + offset.0 + blur) * scale;
    let bottom = (rect.bottom() + offset.1 + blur) * scale;
    let center = [
        (rect.x + rect.width * 0.5 + offset.0) * scale,
        (rect.y + rect.height * 0.5 + offset.1) * scale,
    ];
    let half_size = [rect.width * 0.5 * scale, rect.height * 0.5 * scale];
    let corners = [
        [left, top],
        [right, top],
        [right, bottom],
        [left, bottom],
    ];
    let positions = corners.map(|corner| ndc(corner, size));
    let fill = color.to_array();
    for index in [0u32, 1, 2, 0, 2, 3] {
        let index = index as usize;
        out.push(SolidVertex {
            pos: positions[index],
            pixel: corners[index],
            center,
            half_size,
            radii: [radius * scale; 4],
            fill,
            border_width: 0.0,
            border_color: [0.0; 4],
            blur: (blur * scale).max(0.5),
            mode: 1.0,
        });
    }
}

fn push_texture_quad(
    out: &mut Vec<TextureVertex>,
    rect: Rect,
    uv: [f32; 4],
    color: Color,
    size: Size,
    scale: f32,
) {
    // Snap glyph quads to the physical pixel grid so the atlas is sampled
    // 1:1 (with the half-texel UV inset) instead of being resampled.
    let left = (rect.x * scale).round();
    let top = (rect.y * scale).round();
    let right = (rect.right() * scale).round();
    let bottom = (rect.bottom() * scale).round();
    let corners = [
        [left, top],
        [right, top],
        [right, bottom],
        [left, bottom],
    ];
    let uvs = [
        [uv[0], uv[1]],
        [uv[2], uv[1]],
        [uv[2], uv[3]],
        [uv[0], uv[3]],
    ];
    let positions = corners.map(|corner| ndc(corner, size));
    let color = color.to_array();
    for index in [0u32, 1, 2, 0, 2, 3] {
        let index = index as usize;
        out.push(TextureVertex {
            pos: positions[index],
            uv: uvs[index],
            color,
        });
    }
}
