//! Building the SDF, path, text and image pipelines, plain and multisampled.

use super::{BindGroupLayouts, GpuRenderer, MsaaPipelines, Pipelines};
use crate::path::PathVertex;
use crate::primitives::SdfVertexInstance;
use crate::shaders::{
    PATH_DT_SHADER, PATH_SHADER, SDF_3D_DT_SHADER, SDF_3D_SHADER, SDF_3D_VB_SHADER,
    SDF_CORE_DT_SHADER, SDF_CORE_SHADER, SDF_CORE_VB_SHADER, SDF_NOTCH_DT_SHADER, SDF_NOTCH_SHADER,
    SDF_NOTCH_VB_SHADER, SDF_SHADOW_DT_SHADER, SDF_SHADOW_SHADER, SDF_SHADOW_VB_SHADER,
    TEXT_DT_SHADER,
};

impl GpuRenderer {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn create_pipelines(
        device: &wgpu::Device,
        layouts: &BindGroupLayouts,
        sdf_core_shader: &wgpu::ShaderModule,
        sdf_core_simple_shader: Option<&wgpu::ShaderModule>,
        sdf_simple_shader: Option<&wgpu::ShaderModule>,
        sdf_shadow_shader: &wgpu::ShaderModule,
        sdf_3d_shader: &wgpu::ShaderModule,
        sdf_notch_shader: &wgpu::ShaderModule,
        text_shader: &wgpu::ShaderModule,
        composite_shader: &wgpu::ShaderModule,
        path_shader: &wgpu::ShaderModule,
        layer_composite_shader: &wgpu::ShaderModule,
        clear_quad_shader: &wgpu::ShaderModule,
        texture_format: wgpu::TextureFormat,
        sample_count: u32,
        has_vertex_storage: bool,
        pipeline_cache: Option<&wgpu::PipelineCache>,
    ) -> Pipelines {
        let blend_state = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        };

        let color_targets = &[Some(wgpu::ColorTargetState {
            format: texture_format,
            blend: Some(blend_state),
            write_mask: wgpu::ColorWrites::ALL,
        })];

        let primitive_state = wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        };

        let multisample_state = wgpu::MultisampleState {
            count: sample_count,
            mask: !0,
            alpha_to_coverage_enabled: false,
        };

        // SDF pipeline
        let sdf_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SDF Pipeline Layout"),
            bind_group_layouts: &[&layouts.sdf],
            push_constant_ranges: &[],
        });

        // When VERTEX_STORAGE is unavailable, SDF vertex shaders read from an
        // instance-stepped vertex buffer instead of the storage buffer.
        let sdf_vb_buffers: &[wgpu::VertexBufferLayout<'_>] = if has_vertex_storage {
            &[]
        } else {
            &[SdfVertexInstance::LAYOUT]
        };

        // Overlay pipelines use sample_count=1 for rendering on resolved textures
        let overlay_multisample_state = wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        };

        // --- Split SDF pipelines (share sdf_layout, same blend/primitive state) ---

        // Helper closure to create an SDF pipeline pair (MSAA + overlay) from a shader module
        let make_sdf_pipeline_pair = |shader: &wgpu::ShaderModule,
                                      label: &str,
                                      constants: &[(&str, f64)]|
         -> (wgpu::RenderPipeline, wgpu::RenderPipeline) {
            let fragment_options = wgpu::PipelineCompilationOptions {
                constants,
                ..Default::default()
            };
            let msaa = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&sdf_layout),
                vertex: wgpu::VertexState {
                    module: shader,
                    entry_point: Some("vs_main"),
                    buffers: sdf_vb_buffers,
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: shader,
                    entry_point: Some("fs_main"),
                    targets: color_targets,
                    compilation_options: fragment_options.clone(),
                }),
                primitive: primitive_state,
                depth_stencil: None,
                multisample: multisample_state,
                multiview: None,
                cache: pipeline_cache,
            });
            let overlay_label = format!("{label} Overlay");
            let overlay = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(&overlay_label),
                layout: Some(&sdf_layout),
                vertex: wgpu::VertexState {
                    module: shader,
                    entry_point: Some("vs_main"),
                    buffers: sdf_vb_buffers,
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: shader,
                    entry_point: Some("fs_main"),
                    targets: color_targets,
                    compilation_options: fragment_options.clone(),
                }),
                primitive: primitive_state,
                depth_stencil: None,
                multisample: overlay_multisample_state,
                multiview: None,
                cache: pipeline_cache,
            });
            (msaa, overlay)
        };

        // Build the four SDF split pipeline pairs (8 pipelines) concurrently.
        // Each `create_render_pipeline` is an independent, immutable-borrow
        // driver call, so a scoped thread per pair overlaps the compilation.
        // Build the four SDF pipeline pairs in parallel on native (wgpu's
        // handles are Send+Sync there); Vulkan drivers overlap the compiles,
        // trimming cold first-launch pipeline build. wasm32 wgpu types are
        // !Send (single-threaded), so build serially there.
        // The simple core pipeline is `sdf_core` with the optional blocks off, specialised at build
        // time. Tiers without it share the full pipeline.
        #[cfg(not(target_arch = "wasm32"))]
        let (
            (sdf_core, sdf_core_overlay),
            simple_core,
            plain_core,
            (sdf_shadow, sdf_shadow_overlay),
            (sdf_3d, sdf_3d_overlay),
            (sdf_notch, sdf_notch_overlay),
        ) = std::thread::scope(|s| {
            let h_core =
                s.spawn(|| make_sdf_pipeline_pair(sdf_core_shader, "SDF Core Pipeline", &[]));
            let h_simple = sdf_core_simple_shader.map(|shader| {
                s.spawn(|| make_sdf_pipeline_pair(shader, "SDF Core Simple Pipeline", &[]))
            });
            let h_plain = sdf_simple_shader.map(|shader| {
                s.spawn(|| make_sdf_pipeline_pair(shader, "SDF Plain Pipeline", &[]))
            });
            let h_shadow =
                s.spawn(|| make_sdf_pipeline_pair(sdf_shadow_shader, "SDF Shadow Pipeline", &[]));
            let h_3d = s.spawn(|| make_sdf_pipeline_pair(sdf_3d_shader, "SDF 3D Pipeline", &[]));
            let h_notch =
                s.spawn(|| make_sdf_pipeline_pair(sdf_notch_shader, "SDF Notch Pipeline", &[]));
            (
                h_core.join().unwrap(),
                h_simple.map(|h| h.join().unwrap()),
                h_plain.map(|h| h.join().unwrap()),
                h_shadow.join().unwrap(),
                h_3d.join().unwrap(),
                h_notch.join().unwrap(),
            )
        });

        #[cfg(target_arch = "wasm32")]
        let (
            (sdf_core, sdf_core_overlay),
            simple_core,
            plain_core,
            (sdf_shadow, sdf_shadow_overlay),
            (sdf_3d, sdf_3d_overlay),
            (sdf_notch, sdf_notch_overlay),
        ) = (
            make_sdf_pipeline_pair(sdf_core_shader, "SDF Core Pipeline", &[]),
            sdf_core_simple_shader
                .map(|shader| make_sdf_pipeline_pair(shader, "SDF Core Simple Pipeline", &[])),
            sdf_simple_shader
                .map(|shader| make_sdf_pipeline_pair(shader, "SDF Plain Pipeline", &[])),
            make_sdf_pipeline_pair(sdf_shadow_shader, "SDF Shadow Pipeline", &[]),
            make_sdf_pipeline_pair(sdf_3d_shader, "SDF 3D Pipeline", &[]),
            make_sdf_pipeline_pair(sdf_notch_shader, "SDF Notch Pipeline", &[]),
        );
        let (sdf_core_simple, sdf_core_simple_overlay) =
            simple_core.unwrap_or_else(|| (sdf_core.clone(), sdf_core_overlay.clone()));
        let (sdf_plain, sdf_plain_overlay) = plain_core
            .unwrap_or_else(|| (sdf_core_simple.clone(), sdf_core_simple_overlay.clone()));

        // Text pipeline
        let text_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Text Pipeline Layout"),
            bind_group_layouts: &[&layouts.text],
            push_constant_ranges: &[],
        });

        // TEXT_DT_SHADER has its own vs_main that reads from a glyph data texture
        // (no VB instance attributes needed — unlike SDF DT which uses VB + DT).
        let text_vb_buffers: &[wgpu::VertexBufferLayout<'_>] = &[];

        // Text overlay pipeline - uses sample_count=1 for rendering on resolved textures
        let text_overlay = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Text Overlay Pipeline"),
            layout: Some(&text_layout),
            vertex: wgpu::VertexState {
                module: text_shader,
                entry_point: Some("vs_main"),
                buffers: text_vb_buffers,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: text_shader,
                entry_point: Some("fs_main"),
                targets: color_targets,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: primitive_state,
            depth_stencil: None,
            multisample: overlay_multisample_state,
            multiview: None,
            cache: pipeline_cache,
        });

        // Composite pipeline
        let composite_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Composite Pipeline Layout"),
            bind_group_layouts: &[&layouts.composite],
            push_constant_ranges: &[],
        });

        let composite = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Composite Pipeline"),
            layout: Some(&composite_layout),
            vertex: wgpu::VertexState {
                module: composite_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: composite_shader,
                entry_point: Some("fs_main"),
                targets: color_targets,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: primitive_state,
            depth_stencil: None,
            multisample: multisample_state,
            multiview: None,
            cache: pipeline_cache,
        });

        // Composite overlay pipeline - single-sampled for blending onto resolved textures
        let composite_overlay = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Composite Overlay Pipeline"),
            layout: Some(&composite_layout),
            vertex: wgpu::VertexState {
                module: composite_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: composite_shader,
                entry_point: Some("fs_main"),
                targets: color_targets,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: primitive_state,
            depth_stencil: None,
            multisample: overlay_multisample_state,
            multiview: None,
            cache: pipeline_cache,
        });

        // Path pipeline - uses vertex buffers for tessellated geometry
        let path_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Path Pipeline Layout"),
            bind_group_layouts: &[&layouts.path],
            push_constant_ranges: &[],
        });

        // Vertex buffer layout for PathVertex
        // PathVertex layout (120 bytes total):
        //   position: [f32; 2]       - 8 bytes, offset 0
        //   color: [f32; 4]          - 16 bytes, offset 8
        //   end_color: [f32; 4]      - 16 bytes, offset 24
        //   uv: [f32; 2]             - 8 bytes, offset 40
        //   gradient_params: [f32;4] - 16 bytes, offset 48
        //   gradient_type: u32       - 4 bytes, offset 64
        //   edge_distance: f32       - 4 bytes, offset 68
        //   clip_bounds: [f32;4]     - 16 bytes, offset 72
        //   clip_radius: [f32;4]     - 16 bytes, offset 88
        //   clip_type: u32           - 4 bytes, offset 104
        //   clip_shape: u32          - 4 bytes, offset 108
        //   _padding: [u32; 2]       - 8 bytes, offset 112
        let path_vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<PathVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                // position: vec2<f32>
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 0,
                },
                // color: vec4<f32>
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 8,
                    shader_location: 1,
                },
                // end_color: vec4<f32>
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 24,
                    shader_location: 2,
                },
                // uv: vec2<f32>
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 40,
                    shader_location: 3,
                },
                // gradient_params: vec4<f32>
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 48,
                    shader_location: 4,
                },
                // gradient_type: u32
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Uint32,
                    offset: 64,
                    shader_location: 5,
                },
                // edge_distance: f32 (for anti-aliasing)
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32,
                    offset: 68,
                    shader_location: 6,
                },
                // clip_bounds: vec4<f32>
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 72,
                    shader_location: 7,
                },
                // clip_radius: vec4<f32>
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 88,
                    shader_location: 8,
                },
                // clip_type: u32
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Uint32,
                    offset: 104,
                    shader_location: 9,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Uint32,
                    offset: 108,
                    shader_location: 10,
                },
            ],
        };

        let path = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Path Pipeline"),
            layout: Some(&path_layout),
            vertex: wgpu::VertexState {
                module: path_shader,
                entry_point: Some("vs_main"),
                buffers: std::slice::from_ref(&path_vertex_layout),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: path_shader,
                entry_point: Some("fs_main"),
                targets: color_targets,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: primitive_state,
            depth_stencil: None,
            multisample: multisample_state,
            multiview: None,
            cache: pipeline_cache,
        });

        // Path overlay pipeline - uses sample_count=1 for rendering on resolved textures
        let path_overlay = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Path Overlay Pipeline"),
            layout: Some(&path_layout),
            vertex: wgpu::VertexState {
                module: path_shader,
                entry_point: Some("vs_main"),
                buffers: &[path_vertex_layout],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: path_shader,
                entry_point: Some("fs_main"),
                targets: color_targets,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: primitive_state,
            depth_stencil: None,
            multisample: overlay_multisample_state,
            multiview: None,
            cache: pipeline_cache,
        });

        // Layer composite pipeline - for compositing offscreen layers with blend modes
        let layer_composite_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Layer Composite Pipeline Layout"),
                bind_group_layouts: &[&layouts.layer_composite],
                push_constant_ranges: &[],
            });

        // Use premultiplied alpha blending for layer composition
        let premultiplied_blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        };

        let layer_composite = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Layer Composite Pipeline"),
            layout: Some(&layer_composite_layout),
            vertex: wgpu::VertexState {
                module: layer_composite_shader,
                entry_point: Some("vs_main"),
                buffers: &[], // No vertex buffers - quad generated in shader
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: layer_composite_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: texture_format,
                    blend: Some(premultiplied_blend),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: overlay_multisample_state, // 1x sampled - layers are resolved
            multiview: None,
            cache: pipeline_cache,
        });

        // Compositor v2 scissored clear. No bindings, no vertex
        // buffer — the shader generates a fullscreen triangle from
        // `vertex_index`. REPLACE blend so the fragment's
        // `(0,0,0,0)` output fully overwrites the attachment inside
        // the active scissor rect.
        let clear_quad_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Clear Quad Pipeline Layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });
        let clear_quad = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Clear Quad Pipeline"),
            layout: Some(&clear_quad_layout),
            vertex: wgpu::VertexState {
                module: clear_quad_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: clear_quad_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: texture_format,
                    // REPLACE blend: fragment output replaces dest
                    // verbatim. No multiplicative compositing — we
                    // want the cleared pixels to be exactly
                    // (0,0,0,0), not blended with whatever was there.
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: overlay_multisample_state,
            multiview: None,
            cache: pipeline_cache,
        });

        Pipelines {
            sdf_core,
            sdf_shadow,
            sdf_3d,
            sdf_notch,
            sdf_core_overlay,
            sdf_core_simple,
            sdf_core_simple_overlay,
            sdf_plain,
            sdf_plain_overlay,
            sdf_shadow_overlay,
            sdf_3d_overlay,
            sdf_notch_overlay,
            text_overlay,
            composite,
            composite_overlay,
            path,
            path_overlay,
            layer_composite,
            clear_quad,
        }
    }

    /// Create MSAA-specific pipelines for a given sample count
    pub(super) fn create_msaa_pipelines(
        device: &wgpu::Device,
        layouts: &BindGroupLayouts,
        texture_format: wgpu::TextureFormat,
        sample_count: u32,
        has_vertex_storage: bool,
        has_storage_buffers: bool,
    ) -> MsaaPipelines {
        let blend_state = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        };

        let color_targets = &[Some(wgpu::ColorTargetState {
            format: texture_format,
            blend: Some(blend_state),
            write_mask: wgpu::ColorWrites::ALL,
        })];

        let primitive_state = wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        };

        let multisample_state = wgpu::MultisampleState {
            count: sample_count,
            mask: !0,
            alpha_to_coverage_enabled: false,
        };

        let sdf_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("SDF Pipeline Layout (MSAA)"),
            bind_group_layouts: &[&layouts.sdf],
            push_constant_ranges: &[],
        });

        let sdf_vb_buffers: &[wgpu::VertexBufferLayout<'_>] = if has_vertex_storage {
            &[]
        } else {
            &[SdfVertexInstance::LAYOUT]
        };

        // Split SDF shader modules (MSAA)
        let make_msaa_sdf_pipeline = |source: &str, label: &str| -> wgpu::RenderPipeline {
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&sdf_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: sdf_vb_buffers,
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    targets: color_targets,
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: primitive_state,
                depth_stencil: None,
                multisample: multisample_state,
                multiview: None,
                cache: None,
            })
        };

        let (msaa_core_src, msaa_shadow_src, msaa_3d_src, msaa_notch_src) = if !has_storage_buffers
        {
            (
                SDF_CORE_DT_SHADER,
                SDF_SHADOW_DT_SHADER,
                SDF_3D_DT_SHADER,
                SDF_NOTCH_DT_SHADER,
            )
        } else if !has_vertex_storage {
            (
                SDF_CORE_VB_SHADER,
                SDF_SHADOW_VB_SHADER,
                SDF_3D_VB_SHADER,
                SDF_NOTCH_VB_SHADER,
            )
        } else {
            (
                SDF_CORE_SHADER,
                SDF_SHADOW_SHADER,
                SDF_3D_SHADER,
                SDF_NOTCH_SHADER,
            )
        };

        let sdf_core = make_msaa_sdf_pipeline(msaa_core_src, "SDF Core Pipeline (MSAA)");
        let sdf_shadow = make_msaa_sdf_pipeline(msaa_shadow_src, "SDF Shadow Pipeline (MSAA)");
        let sdf_3d = make_msaa_sdf_pipeline(msaa_3d_src, "SDF 3D Pipeline (MSAA)");
        let sdf_notch = make_msaa_sdf_pipeline(msaa_notch_src, "SDF Notch Pipeline (MSAA)");

        // Create path shader
        let path_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Path Shader (MSAA)"),
            source: wgpu::ShaderSource::Wgsl(
                if has_storage_buffers {
                    PATH_SHADER
                } else {
                    PATH_DT_SHADER
                }
                .into(),
            ),
        });

        let path_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Path Pipeline Layout (MSAA)"),
            bind_group_layouts: &[&layouts.path],
            push_constant_ranges: &[],
        });

        // PathVertex layout — see PathVertex struct in path.rs for offset rationale
        let path_vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<PathVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 0,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 8,
                    shader_location: 1,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 24,
                    shader_location: 2,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 40,
                    shader_location: 3,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 48,
                    shader_location: 4,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Uint32,
                    offset: 64,
                    shader_location: 5,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32,
                    offset: 68,
                    shader_location: 6,
                },
                // clip_bounds: vec4<f32>
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 72,
                    shader_location: 7,
                },
                // clip_radius: vec4<f32>
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 88,
                    shader_location: 8,
                },
                // clip_type: u32
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Uint32,
                    offset: 104,
                    shader_location: 9,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Uint32,
                    offset: 108,
                    shader_location: 10,
                },
            ],
        };

        let path = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Path Pipeline (MSAA)"),
            layout: Some(&path_layout),
            vertex: wgpu::VertexState {
                module: &path_shader,
                entry_point: Some("vs_main"),
                buffers: &[path_vertex_layout],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &path_shader,
                entry_point: Some("fs_main"),
                targets: color_targets,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: primitive_state,
            depth_stencil: None,
            multisample: multisample_state,
            multiview: None,
            cache: None,
        });

        MsaaPipelines {
            sdf_core,
            sdf_shadow,
            sdf_3d,
            sdf_notch,
            path,
            sample_count,
        }
    }
}
