//! Requesting the adapter and device and assembling a renderer around them.

use super::{
    EffectPipelines, GpuMemoryBudget, GpuRenderer, LayerTextureCache, RendererConfig,
    RendererError, apply_renderer_config_overrides, env_usize, load_pipeline_cache_data,
    log_renderer_config, requested_device_features, save_pipeline_cache_data,
};
use crate::gradient_texture::GradientTextureCache;
use crate::shaders::{
    CLEAR_QUAD_SHADER, COMPOSITE_SHADER, LAYER_COMPOSITE_SHADER, PATH_DT_SHADER, PATH_SHADER,
    SDF_3D_DT_SHADER, SDF_3D_SHADER, SDF_3D_VB_SHADER, SDF_CORE_DT_SHADER, SDF_CORE_SHADER,
    SDF_CORE_VB_SHADER, SDF_NOTCH_DT_SHADER, SDF_NOTCH_SHADER, SDF_NOTCH_VB_SHADER,
    SDF_SHADOW_DT_SHADER, SDF_SHADOW_SHADER, SDF_SHADOW_VB_SHADER, TEXT_DT_SHADER, TEXT_SHADER,
};
use std::collections::HashMap;
use std::sync::Arc;

impl GpuRenderer {
    pub(super) fn create_renderer(
        instance: wgpu::Instance,
        adapter: wgpu::Adapter,
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        texture_format: wgpu::TextureFormat,
        mut config: RendererConfig,
        viewport_size: (u32, u32),
    ) -> Result<Self, RendererError> {
        // Check if the adapter supports storage buffers in vertex shaders
        let has_vertex_storage = adapter
            .get_downlevel_capabilities()
            .flags
            .contains(wgpu::DownlevelFlags::VERTEX_STORAGE);
        if !has_vertex_storage {
            tracing::info!("VERTEX_STORAGE not supported — using instance vertex buffer fallback");
        }

        // Check if the device enabled BC texture compression (requested
        // via `requested_device_features` earlier; we check the
        // *device* features, not the adapter's, because the device
        // may have been opened with a subset).
        let has_texture_compression_bc = device
            .features()
            .contains(wgpu::Features::TEXTURE_COMPRESSION_BC);
        if has_texture_compression_bc {
            tracing::info!(
                "BC texture compression enabled — material textures upload as BC1/3/4/5"
            );
        } else {
            tracing::info!(
                "BC texture compression unavailable on this adapter — material textures upload as Rgba8"
            );
        }

        // Check if the adapter supports storage buffers at all (Tier 3 / DT fallback)
        let has_storage_buffers = adapter.limits().max_storage_buffers_per_shader_stage > 0;
        if !has_storage_buffers {
            tracing::info!("No storage buffer support — using data texture fallback (WebGL2 mode)");
            // When there are no storage buffers, max_storage_buffer_binding_size is 0,
            // so apply_renderer_config_overrides clamped max_primitives/max_glyphs to 1.
            // Re-apply sensible defaults clamped by texture dimension limits instead.
            let tex_max = adapter.limits().max_texture_dimension_2d as usize;
            let defaults = RendererConfig::default();
            // Use env overrides if present, otherwise fall back to defaults
            config.max_primitives = env_usize("BLINC_GPU_MAX_PRIMITIVES")
                .unwrap_or(defaults.max_primitives)
                .clamp(1, tex_max);
            config.max_glyphs = env_usize("BLINC_GPU_MAX_GLYPHS")
                .unwrap_or(defaults.max_glyphs)
                .clamp(1, tex_max);
            log_renderer_config(&config);
        }

        // Create bind group layouts
        let bind_group_layouts = Self::create_bind_group_layouts_with_flags(
            &device,
            has_vertex_storage,
            has_storage_buffers,
        );

        // Create shaders

        let text_source = if has_storage_buffers {
            TEXT_SHADER
        } else {
            TEXT_DT_SHADER
        };
        let text_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Text Shader"),
            source: wgpu::ShaderSource::Wgsl(text_source.into()),
        });

        let composite_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Composite Shader"),
            source: wgpu::ShaderSource::Wgsl(COMPOSITE_SHADER.into()),
        });

        let path_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Path Shader"),
            source: wgpu::ShaderSource::Wgsl(
                if has_storage_buffers {
                    PATH_SHADER
                } else {
                    PATH_DT_SHADER
                }
                .into(),
            ),
        });

        let layer_composite_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Layer Composite Shader"),
            source: wgpu::ShaderSource::Wgsl(LAYER_COMPOSITE_SHADER.into()),
        });

        let clear_quad_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Clear Quad Shader"),
            source: wgpu::ShaderSource::Wgsl(CLEAR_QUAD_SHADER.into()),
        });

        // Split SDF shaders — specialized pipelines for each primitive category.
        // Three tiers:
        //   Tier 1 (full): Storage buffers in VS + FS — sdf_*.wgsl
        //   Tier 2 (VB):   No VS storage, FS storage works — sdf_*_vb.wgsl + vertex buffer
        //   Tier 3 (DT):   No storage at all (WebGL2) — sdf_*_dt.wgsl + VB + data textures
        let (sdf_core_source, sdf_shadow_source, sdf_3d_source, sdf_notch_source) =
            if !has_storage_buffers {
                // Tier 3: Data texture fallback (no storage buffers at all)
                (
                    SDF_CORE_DT_SHADER,
                    SDF_SHADOW_DT_SHADER,
                    SDF_3D_DT_SHADER,
                    SDF_NOTCH_DT_SHADER,
                )
            } else if !has_vertex_storage {
                // Tier 2: VB fallback (no VS storage, FS storage works)
                (
                    SDF_CORE_VB_SHADER,
                    SDF_SHADOW_VB_SHADER,
                    SDF_3D_VB_SHADER,
                    SDF_NOTCH_VB_SHADER,
                )
            } else {
                // Tier 1: Full storage buffer support
                (
                    SDF_CORE_SHADER,
                    SDF_SHADOW_SHADER,
                    SDF_3D_SHADER,
                    SDF_NOTCH_SHADER,
                )
            };

        let sdf_core_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SDF Core Shader"),
            source: wgpu::ShaderSource::Wgsl(sdf_core_source.into()),
        });
        // `sdf_core` with the optional blocks off, specialised when the crate is built. Like the
        // compact shader below it reads the primitive in the vertex stage: full tier only.
        let sdf_core_simple_shader = has_vertex_storage.then(|| {
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("SDF Core Simple Shader"),
                source: wgpu::ShaderSource::Wgsl(crate::sdf_russl::simple::WGSL.into()),
            })
        });
        // The compact shader reads the primitive in the vertex stage, so it needs the full tier.
        let sdf_simple_shader = has_vertex_storage.then(|| {
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("SDF Simple Shader"),
                source: wgpu::ShaderSource::Wgsl(crate::shaders::SDF_SIMPLE_SHADER.into()),
            })
        });
        let sdf_shadow_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SDF Shadow Shader"),
            source: wgpu::ShaderSource::Wgsl(sdf_shadow_source.into()),
        });
        let sdf_3d_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SDF 3D Shader"),
            source: wgpu::ShaderSource::Wgsl(sdf_3d_source.into()),
        });
        let sdf_notch_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("SDF Notch Shader"),
            source: wgpu::ShaderSource::Wgsl(sdf_notch_source.into()),
        });

        // Persisted pipeline cache: seed from disk (validated against this
        // adapter), feed it to pipeline creation, then write it back. Skips
        // the bulk of pipeline (re)build on every launch after the first.
        let adapter_info = adapter.get_info();
        let pipeline_cache = if device.features().contains(wgpu::Features::PIPELINE_CACHE) {
            let data = config
                .pipeline_cache_path
                .as_deref()
                .and_then(|p| load_pipeline_cache_data(p, &adapter_info));
            tracing::info!(
                "Pipeline cache: {} ({} bytes)",
                if data.is_some() { "loaded" } else { "cold" },
                data.as_deref().map(<[u8]>::len).unwrap_or(0),
            );
            // SAFETY: `data`, when Some, is bytes we previously produced via
            // `PipelineCache::get_data` and validated against this exact
            // adapter in `load_pipeline_cache_data`. `fallback: true` also
            // makes the driver ignore data it can't use.
            Some(unsafe {
                device.create_pipeline_cache(&wgpu::PipelineCacheDescriptor {
                    label: Some("Blinc Pipeline Cache"),
                    data: data.as_deref(),
                    fallback: true,
                })
            })
        } else {
            None
        };

        // Create pipelines (core only — effect pipelines are lazy)
        let pipelines = Self::create_pipelines(
            &device,
            &bind_group_layouts,
            &sdf_core_shader,
            sdf_core_simple_shader.as_ref(),
            sdf_simple_shader.as_ref(),
            &sdf_shadow_shader,
            &sdf_3d_shader,
            &sdf_notch_shader,
            &text_shader,
            &composite_shader,
            &path_shader,
            &layer_composite_shader,
            &clear_quad_shader,
            texture_format,
            config.sample_count,
            has_vertex_storage,
            pipeline_cache.as_ref(),
        );

        // Persist the (now-populated) cache for the next launch. Best-effort.
        if let (Some(cache), Some(path)) = (
            pipeline_cache.as_ref(),
            config.pipeline_cache_path.as_deref(),
        ) && let Some(data) = cache.get_data()
        {
            tracing::info!("Pipeline cache: saving {} bytes", data.len());
            save_pipeline_cache_data(path, &adapter_info, &data);
        }

        // Create buffers (storage buffers always created; DT textures added when needed)
        let buffers = Self::create_buffers(&device, &config, has_storage_buffers);

        // Create placeholder glyph atlas textures (1x1 transparent)
        // These are used when no text is rendered, satisfying the bind group layout
        let placeholder_glyph_atlas = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Placeholder Glyph Atlas"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm, // Grayscale for regular glyphs
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let placeholder_glyph_atlas_view =
            placeholder_glyph_atlas.create_view(&wgpu::TextureViewDescriptor::default());

        let placeholder_color_glyph_atlas = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Placeholder Color Glyph Atlas"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb, // RGBA for color emoji
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let placeholder_color_glyph_atlas_view =
            placeholder_color_glyph_atlas.create_view(&wgpu::TextureViewDescriptor::default());

        // Create sampler for glyph atlases
        let glyph_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Glyph Atlas Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // Create gradient texture cache for multi-stop gradients on paths
        let gradient_texture_cache = GradientTextureCache::new(&device, &queue);

        // Create placeholder image texture for paths (1x1 white)
        let placeholder_path_image = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Placeholder Path Image"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        // Initialize with white pixel
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &placeholder_path_image,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &[255u8, 255, 255, 255], // White pixel
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        let placeholder_path_image_view =
            placeholder_path_image.create_view(&wgpu::TextureViewDescriptor::default());

        // Create sampler for path image textures
        let path_image_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Path Image Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // Create 1x1 dummy texture for blend mode dest binding (Normal mode doesn't read it)
        let dummy_blend_dest = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Dummy Blend Dest"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &dummy_blend_dest,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &[0u8, 0, 0, 0], // Transparent pixel
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        let dummy_blend_dest_view =
            dummy_blend_dest.create_view(&wgpu::TextureViewDescriptor::default());

        // Create initial bind groups
        let bind_groups = Self::create_bind_groups(
            &device,
            &bind_group_layouts,
            &buffers,
            &placeholder_glyph_atlas_view,
            &placeholder_color_glyph_atlas_view,
            &glyph_sampler,
            &gradient_texture_cache,
            &placeholder_path_image_view,
            &path_image_sampler,
            has_storage_buffers,
        );

        let flow_pipeline_cache =
            crate::flow_pipeline::FlowPipelineCache::new(device.clone(), texture_format);

        Ok(Self {
            instance,
            adapter,
            device,
            queue,
            pipelines,
            effect_pipelines: EffectPipelines {
                blur: None,
                color_matrix: None,
                drop_shadow: None,
                glow: None,
                mask_image: None,
                glass: None,
                simple_glass: None,
            },
            msaa_pipelines: None,
            buffers,
            bind_groups,
            bind_group_layouts,
            viewport_size,
            saved_viewport_size: None,
            pending_scissor: None,
            pending_damage_scissor: None,
            memory_budget: GpuMemoryBudget::new(config.gpu_memory_budget),
            config,
            time: 0.0,
            texture_format,
            image_pipeline: None,
            mesh_pipeline: None,
            custom_passes: crate::custom_pass::CustomPassManager::new(),
            overlay_targets: super::overlay_targets::OverlayTargets::default(),
            overlay_alpha: None,
            cached_glass: None,
            placeholder_glyph_atlas_view,
            placeholder_color_glyph_atlas_view,
            glyph_sampler,
            active_glyph_atlas: None,
            gradient_texture_cache,
            placeholder_path_image_view,
            path_image_sampler,
            layer_texture_cache: LayerTextureCache::new(texture_format),
            sdf_3d_resources: None,
            particle_systems: std::collections::HashMap::new(),
            mask_image_cache: HashMap::new(),
            dummy_blend_dest_view,
            dummy_blend_dest_texture: dummy_blend_dest,
            blend_target_ptr: None,
            flow_pipeline_cache,
            flow_cursor: [0.0; 2],
            scene_copy_texture: None,
            scene_copy_last_frame: 0,
            mesh_targets_last_frame: 0,
            has_vertex_storage,
            has_storage_buffers,
            has_texture_compression_bc,
            static_layer: None,
        })
    }

    /// Create a surface from platform raw handles using the renderer's
    /// existing instance.
    ///
    /// This is used by platforms such as Android where the OS can destroy
    /// and recreate the native window while the GPU device and render tree
    /// should remain alive.
    ///
    /// # Safety
    ///
    /// The caller must uphold [`wgpu::Instance::create_surface_unsafe`]'s
    /// handle validity and lifetime requirements.
    pub unsafe fn create_surface_unsafe(
        &self,
        target: wgpu::SurfaceTargetUnsafe,
    ) -> Result<wgpu::Surface<'static>, RendererError> {
        unsafe { self.instance.create_surface_unsafe(target) }.map_err(RendererError::SurfaceError)
    }
}
