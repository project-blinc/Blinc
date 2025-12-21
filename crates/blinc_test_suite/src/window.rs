//! Interactive window tests
//!
//! This module provides utilities for interactive testing with live windows.
//! Only available when the `interactive` feature is enabled.

use anyhow::Result;
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

/// Run an interactive test window
pub fn run_interactive_test<F>(title: &str, render_fn: F) -> Result<()>
where
    F: Fn(&wgpu::Device, &wgpu::Queue, &wgpu::TextureView) + 'static,
{
    let event_loop = EventLoop::new()?;
    let window = WindowBuilder::new()
        .with_title(title)
        .with_inner_size(winit::dpi::LogicalSize::new(800, 600))
        .build(&event_loop)?;

    let instance = wgpu::Instance::default();
    let surface = instance.create_surface(&window)?;

    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::default(),
        compatible_surface: Some(&surface),
        force_fallback_adapter: false,
    }))
    .ok_or_else(|| anyhow::anyhow!("Failed to find adapter"))?;

    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("Interactive Test Device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::default(),
        },
        None,
    ))?;

    let size = window.inner_size();
    let mut config = surface
        .get_default_config(&adapter, size.width, size.height)
        .ok_or_else(|| anyhow::anyhow!("Surface not supported"))?;
    config.present_mode = wgpu::PresentMode::AutoVsync;
    surface.configure(&device, &config);

    event_loop.run(move |event, elwt| {
        elwt.set_control_flow(ControlFlow::Wait);

        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    elwt.exit();
                }
                WindowEvent::Resized(new_size) => {
                    if new_size.width > 0 && new_size.height > 0 {
                        config.width = new_size.width;
                        config.height = new_size.height;
                        surface.configure(&device, &config);
                        window.request_redraw();
                    }
                }
                WindowEvent::RedrawRequested => {
                    if let Ok(frame) = surface.get_current_texture() {
                        let view = frame
                            .texture
                            .create_view(&wgpu::TextureViewDescriptor::default());
                        render_fn(&device, &queue, &view);
                        frame.present();
                    }
                }
                _ => {}
            },
            _ => {}
        }
    })?;

    Ok(())
}
