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

use dear_imgui_rs::Context;
use dear_imgui_wgpu::wgpu;
use dear_imgui_wgpu::{FramebufferExtent, WgpuInitInfo, WgpuRenderer};
use dear_imgui_winit::{HiDpiMode, WinitPlatform};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

type AnyError = Box<dyn std::error::Error>;

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
        let consumer = self.renderer.renderer_consumer()?;
        let frame = {
            let ui = &*self.context.frame();
            ui.window("dear-imgui-rs 0.22")
                .size([460.0, 96.0], dear_imgui_rs::Condition::FirstUseEver)
                .build(|| {
                    ui.text("Dear ImGui 1.93.1 via dear-imgui-rs 0.22.0");
                    ui.text("winit + wgpu on native and wasm32");
                    ui.text(format!("frame {frames}"));
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
