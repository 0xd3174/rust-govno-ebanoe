//! Minimal dear-imgui-rs 0.22 demo shared by the native (winit) and wasm32 targets.
//!
//! The wasm build requires the `imgui-sys-v1` provider module; see README.md.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

use dear_imgui_rs::{Color, Condition, Context, Ui, WindowFlags};
use dear_imgui_wgpu::wgpu;
use dear_imgui_wgpu::{FramebufferExtent, WgpuInitInfo, WgpuRenderer};
use dear_imgui_winit::{HiDpiMode, WinitPlatform};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

type AnyError = Box<dyn std::error::Error>;

const TORUS_MAJOR: f32 = 1.0;
const TORUS_MINOR: f32 = 0.42;
const TORUS_THETA: usize = 72;
const TORUS_PHI: usize = 36;

#[cfg(test)]
fn torus_point(theta: f32, phi: f32) -> [f32; 3] {
    let ring = TORUS_MAJOR + TORUS_MINOR * phi.cos();
    [ring * theta.cos(), TORUS_MINOR * phi.sin(), ring * theta.sin()]
}

#[cfg(test)]
fn torus_normal(theta: f32, phi: f32) -> [f32; 3] {
    [
        phi.cos() * theta.cos(),
        phi.sin(),
        phi.cos() * theta.sin(),
    ]
}

#[cfg(test)]
fn rotate_x([x, y, z]: [f32; 3], angle: f32) -> [f32; 3] {
    let (sin, cos) = angle.sin_cos();
    [x, y * cos - z * sin, y * sin + z * cos]
}

#[cfg(test)]
fn rotate_y([x, y, z]: [f32; 3], angle: f32) -> [f32; 3] {
    let (sin, cos) = angle.sin_cos();
    [x * cos + z * sin, y, -x * sin + z * cos]
}

#[inline]
fn rotate_x_cached([x, y, z]: [f32; 3], sin: f32, cos: f32) -> [f32; 3] {
    [x, y * cos - z * sin, y * sin + z * cos]
}

#[inline]
fn rotate_y_cached([x, y, z]: [f32; 3], sin: f32, cos: f32) -> [f32; 3] {
    [x * cos + z * sin, y, -x * sin + z * cos]
}

/// Spinning torus rendered through the ImGui draw list. This exercises the full
/// draw-list -> renderer path on both native and wasm without extra GPU pipelines.
fn draw_torus(ui: &Ui, time: f32) {
    let [width, height] = ui.io().display_size();
    if width < 64.0 || height < 64.0 {
        return;
    }
    let center = [width * 0.5, height * 0.5];
    let scale = width.min(height) * 0.30;
    let (rot_x_sin, rot_x_cos) = (time * 0.55).sin_cos();
    let (rot_y_sin, rot_y_cos) = (time * 0.9).sin_cos();
    let light = [-0.45_f32, 0.55, -0.70];
    let theta_table: Vec<(f32, f32)> = (0..TORUS_THETA)
        .map(|index| {
            let theta = index as f32 / TORUS_THETA as f32 * std::f32::consts::TAU;
            theta.sin_cos()
        })
        .collect();
    let phi_table: Vec<(f32, f32)> = (0..TORUS_PHI)
        .map(|index| {
            let phi = index as f32 / TORUS_PHI as f32 * std::f32::consts::TAU;
            phi.sin_cos()
        })
        .collect();

    let mut dots = Vec::with_capacity(TORUS_THETA * TORUS_PHI);
    for &(theta_sin, theta_cos) in &theta_table {
        for &(phi_sin, phi_cos) in &phi_table {
            let point = rotate_y_cached(
                rotate_x_cached(
                    [
                        (TORUS_MAJOR + TORUS_MINOR * phi_cos) * theta_cos,
                        TORUS_MINOR * phi_sin,
                        (TORUS_MAJOR + TORUS_MINOR * phi_cos) * theta_sin,
                    ],
                    rot_x_sin,
                    rot_x_cos,
                ),
                rot_y_sin,
                rot_y_cos,
            );
            let normal = rotate_y_cached(
                rotate_x_cached(
                    [phi_cos * theta_cos, phi_sin, phi_cos * theta_sin],
                    rot_x_sin,
                    rot_x_cos,
                ),
                rot_y_sin,
                rot_y_cos,
            );
            let perspective = 3.2 / (point[2] + 4.2);
            let light_dot = (normal[0] * light[0]
                + normal[1] * light[1]
                + normal[2] * light[2])
                .max(0.0);
            let shade = 0.32 + 0.68 * light_dot.powf(1.3);
            dots.push((
                [
                    center[0] + point[0] * perspective * scale,
                    center[1] - point[1] * perspective * scale,
                ],
                point[2],
                shade,
            ));
        }
    }
    dots.sort_by(|left, right| left.1.total_cmp(&right.1));

    let draw_list = ui.get_window_draw_list();
    for ([x, y], _, shade) in dots {
        let color = Color::from_rgba_bytes(
            (255.0 * shade) as u8,
            (170.0 * shade) as u8,
            (55.0 * shade) as u8,
            245,
        );
        draw_list
            .add_circle([x, y], 1.3 + 0.7 * shade, color)
            .filled(true)
            .num_segments(std::num::NonZeroUsize::new(6).unwrap())
            .build();
    }
}

struct DemoState {
    context: Context,
    platform: WinitPlatform,
    renderer: WgpuRenderer,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    device: wgpu::Device,
    queue: wgpu::Queue,
    window: Arc<Window>,
    frames: u64,
    last_frame_at: Instant,
    started_at: Instant,
}

impl DemoState {
    async fn new(window: Arc<Window>) -> Result<Self, AnyError> {
        let mut context = Context::create();
        context.set_ini_filename(None::<PathBuf>)?;
        let mut platform = WinitPlatform::new(&mut context)?;
        platform.attach_window(window.clone(), HiDpiMode::Default, &mut context)?;

        let mut instance_desc = wgpu::InstanceDescriptor::new_without_display_handle();
        // Browsers may expose `navigator.gpu` without a usable adapter. Force the
        // WebGL backend there; native builds keep the default backend set.
        if cfg!(target_arch = "wasm32") {
            instance_desc.backends = wgpu::Backends::GL;
        }
        let instance = wgpu::Instance::new(instance_desc);
        let surface = instance.create_surface(window.clone())?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await?;
        let limits = if cfg!(target_arch = "wasm32") {
            wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits())
        } else {
            wgpu::Limits::default()
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_limits: limits,
                ..Default::default()
            })
            .await?;

        let size = window.inner_size();
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: caps.alpha_modes[0],
            view_formats: Vec::new(),
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let renderer = WgpuRenderer::new(
            WgpuInitInfo::new(device.clone(), queue.clone(), format),
            &mut context,
        )?;

        Ok(Self {
            context,
            platform,
            renderer,
            surface,
            config,
            device,
            queue,
            window,
            frames: 0,
            last_frame_at: Instant::now(),
            started_at: Instant::now(),
        })
    }

    fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
    }

    fn redraw(&mut self) -> Result<(), AnyError> {
        let size = self.window.inner_size();
        if size.width == 0 || size.height == 0 {
            log::debug!("redraw skipped: window size {size:?}");
            return Ok(());
        }
        let now = Instant::now();
        if now.duration_since(self.last_frame_at) < Duration::from_millis(1) {
            return Ok(());
        }
        self.last_frame_at = now;
        if self.frames == 0 {
            log::info!("first frame: {}x{}", size.width, size.height);
        }
        if (self.config.width, self.config.height) != (size.width, size.height) {
            self.resize(size);
        }

        self.platform
            .prepare_frame(&mut self.context, &self.window)?;

        let frames = self.frames;
        if frames == 0 {
            self.started_at = Instant::now();
        }
        let elapsed = self.started_at.elapsed().as_secs_f32();
        let consumer = self.renderer.renderer_consumer()?;
        let frame = {
            let ui = &*self.context.frame();
            let [width, height] = ui.io().display_size();
            let flags = WindowFlags::NO_DECORATION
                | WindowFlags::NO_BACKGROUND
                | WindowFlags::NO_MOVE
                | WindowFlags::NO_RESIZE
                | WindowFlags::NO_SCROLLBAR
                | WindowFlags::NO_SAVED_SETTINGS
                | WindowFlags::NO_INPUTS
                | WindowFlags::NO_FOCUS_ON_APPEARING
                | WindowFlags::NO_NAV;
            ui.window("canvas")
                .size([width, height], Condition::Always)
                .position([0.0, 0.0], Condition::Always)
                .flags(flags)
                .build(|| {
                    draw_torus(ui, elapsed);
                    ui.get_window_draw_list()
                        .add_rect(
                            [8.0, 8.0],
                            [600.0, 86.0],
                            Color::from_rgba_bytes(8, 8, 14, 210),
                        )
                        .filled(true)
                        .build();
                    ui.set_cursor_pos([16.0, 14.0]);
                    ui.text("Dear ImGui 1.93.1 via dear-imgui-rs 0.22.0");
                    ui.text("winit + wgpu on native and wasm32");
                    let fps = frames as f32 / elapsed.max(0.001);
                    ui.text(format!(
                        "frame {frames} · {fps:.0} fps · torus {} dots",
                        TORUS_THETA * TORUS_PHI
                    ));
                });
            self.context.render(consumer)
        };

        let surface_texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config);
                return Ok(());
            }
            other => return Err(format!("surface acquisition failed: {other:?}").into()),
        };
        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("imgui-demo"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("imgui-demo"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.05,
                            g: 0.05,
                            b: 0.08,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                multiview_mask: None,
                occlusion_query_set: None,
            });
            self.renderer.render(
                frame,
                &mut pass,
                FramebufferExtent::from_texture(&surface_texture.texture),
            )?;
        }
        self.queue.submit(Some(encoder.finish()));
        self.queue.present(surface_texture);

        self.frames += 1;
        Ok(())
    }
}

impl Drop for DemoState {
    fn drop(&mut self) {
        let _ = self.renderer.shutdown(&mut self.context);
    }
}

pub struct DemoApp {
    state: Rc<RefCell<Option<DemoState>>>,
}

impl DemoApp {
    pub fn new() -> Self {
        Self {
            state: Rc::new(RefCell::new(None)),
        }
    }
}

impl ApplicationHandler for DemoApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.borrow().is_some() {
            return;
        }
        #[allow(unused_mut)]
        let mut attributes = Window::default_attributes()
            .with_title("dear-imgui-rs 0.22 - winit + wgpu")
            .with_inner_size(LogicalSize::new(800.0, 500.0));
        #[cfg(target_arch = "wasm32")]
        {
            use winit::platform::web::WindowAttributesExtWebSys;
            attributes = attributes.with_append(true);
        }
        let window = Arc::new(event_loop.create_window(attributes).expect("create window"));
        log::debug!("winit: window created ({:?})", window.inner_size());

        #[cfg(not(target_arch = "wasm32"))]
        {
            let state = pollster::block_on(DemoState::new(window)).expect("initialize wgpu");
            state.window.request_redraw();
            *self.state.borrow_mut() = Some(state);
        }

        #[cfg(target_arch = "wasm32")]
        {
            let slot = Rc::clone(&self.state);
            wasm_bindgen_futures::spawn_local(async move {
                match DemoState::new(window).await {
                    Ok(state) => {
                        state.window.request_redraw();
                        *slot.borrow_mut() = Some(state);
                    }
                    Err(error) => {
                        log::error!("initialization failed: {error}");
                        let message = format!("initialization failed: {error}");
                        web_sys::console::error_1(&wasm_bindgen::JsValue::from_str(&message));
                    }
                }
            });
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(mut state) = self.state.borrow_mut().take() else {
            return;
        };

        if let Err(error) = state
            .platform
            .handle_window_event(&mut state.context, state.window.id(), &event)
        {
            log::error!("winit event handling failed: {error}");
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                if let Err(error) = state.redraw() {
                    log::error!("redraw failed: {error}");
                    event_loop.exit();
                }
                state.window.request_redraw();
            }
            WindowEvent::Resized(size) => state.resize(size),
            WindowEvent::ScaleFactorChanged { .. } => {
                let size = state.window.inner_size();
                state.resize(size);
            }
            _ => {}
        }

        *self.state.borrow_mut() = Some(state);
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(state) = self.state.borrow().as_ref() {
            state.window.request_redraw();
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn run_native() -> Result<(), AnyError> {
    env_logger::init();
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
    let mut app = DemoApp::new();
    event_loop.run_app(&mut app)?;
    Ok(())
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() -> Result<(), wasm_bindgen::JsValue> {
    use winit::platform::web::EventLoopExtWebSys;

    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(log::Level::Info);
    log::info!("wasm start: creating event loop");
    let event_loop =
        EventLoop::new().map_err(|error| wasm_bindgen::JsValue::from_str(&error.to_string()))?;
    log::info!("wasm start: spawning app");
    event_loop.spawn_app(DemoApp::new());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn torus_surface_points_and_normals_are_bounded() {
        for i in 0..32 {
            let theta = i as f32 / 32.0 * std::f32::consts::TAU;
            for j in 0..16 {
                let phi = j as f32 / 16.0 * std::f32::consts::TAU;
                let point = torus_point(theta, phi);
                let ring = (point[0] * point[0] + point[2] * point[2]).sqrt();
                assert!(ring >= TORUS_MAJOR - TORUS_MINOR - 1e-4);
                assert!(ring <= TORUS_MAJOR + TORUS_MINOR + 1e-4);
                assert!(point[1].abs() <= TORUS_MINOR + 1e-4);

                let normal = torus_normal(theta, phi);
                let length =
                    (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2])
                        .sqrt();
                assert!((length - 1.0).abs() < 1e-4);
            }
        }
    }

    #[test]
    fn rotations_preserve_length() {
        let point = torus_point(0.3, 1.7);
        let length = |value: [f32; 3]| {
            (value[0] * value[0] + value[1] * value[1] + value[2] * value[2]).sqrt()
        };
        let rotated = rotate_y(rotate_x(point, 0.7), 1.3);
        assert!((length(point) - length(rotated)).abs() < 1e-4);
    }
}
