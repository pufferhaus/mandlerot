//! Desktop GL context via winit + glutin. Used for macOS/Linux dev.
//!
//! Optionally hosts a second "status preview" window rendered via softbuffer
//! (CPU pixel push, no GL). Enabled by calling `enable_status_window` before
//! the main loop starts.

use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use glutin::config::ConfigTemplateBuilder;
use glutin::context::{ContextApi, ContextAttributesBuilder, Version};
use glutin::display::GetGlDisplay;
use glutin::prelude::*;
use glutin::surface::{Surface, SurfaceAttributesBuilder, WindowSurface};
use glutin_winit::DisplayBuilder;
use winit::dpi::PhysicalSize;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::raw_window_handle::HasWindowHandle;
use winit::window::{Window, WindowId};

use crate::error::{Error, Result};

use super::target::{assert_gles2_capable, RenderTarget};

// ── softbuffer status window ──────────────────────────────────────────────────

/// Width/height of the status preview window (2× the 480×320 panel).
const STATUS_WIN_W: u32 = 960;
const STATUS_WIN_H: u32 = 640;
/// Source panel dimensions.
const PANEL_W: u32 = 480;
const PANEL_H: u32 = 320;

/// Holds the resources for the live status preview window.
struct StatusWindow {
    window: Rc<Window>,
    /// The softbuffer context must be kept alive for the surface to remain valid.
    #[allow(dead_code)]
    context: softbuffer::Context<Rc<Window>>,
    surface: softbuffer::Surface<Rc<Window>, Rc<Window>>,
    /// Shared RGB565 buffer written by the status thread.
    buf: Arc<Mutex<Vec<u16>>>,
}

impl StatusWindow {
    /// Create the window + softbuffer surface, called from inside the winit
    /// event-loop closure where `ActiveEventLoop` is available. Positions the
    /// new window immediately to the right of `main_window` if the platform
    /// reports a position for it.
    fn create(
        event_loop: &winit::event_loop::ActiveEventLoop,
        buf: Arc<Mutex<Vec<u16>>>,
        main_window: &Window,
    ) -> std::result::Result<Self, String> {
        let mut attrs = Window::default_attributes()
            .with_inner_size(PhysicalSize::new(STATUS_WIN_W, STATUS_WIN_H))
            .with_resizable(false)
            .with_title("mandleROT — Status");
        if let Ok(pos) = main_window.outer_position() {
            let size = main_window.outer_size();
            // Place to the right of the main window with an 8px gap.
            let x = pos.x + size.width as i32 + 8;
            let y = pos.y;
            attrs = attrs.with_position(winit::dpi::PhysicalPosition::new(x, y));
        }
        let window = Rc::new(
            event_loop
                .create_window(attrs)
                .map_err(|e| format!("status window: {e}"))?,
        );
        let context = softbuffer::Context::new(window.clone())
            .map_err(|e| format!("softbuffer context: {e}"))?;
        let mut surface = softbuffer::Surface::new(&context, window.clone())
            .map_err(|e| format!("softbuffer surface: {e}"))?;
        surface
            .resize(
                NonZeroU32::new(STATUS_WIN_W).unwrap(),
                NonZeroU32::new(STATUS_WIN_H).unwrap(),
            )
            .map_err(|e| format!("softbuffer resize: {e}"))?;
        Ok(Self {
            window,
            context,
            surface,
            buf,
        })
    }

    /// Paint the current RGB565 buffer into the softbuffer surface at 2× scale.
    fn paint(&mut self) {
        // Sample from shared buffer without holding the lock during present.
        let src: Vec<u16> = {
            let guard = self.buf.lock().unwrap();
            guard.clone()
        };
        let mut sb_buf = match self.surface.buffer_mut() {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("status paint buffer_mut: {e}");
                return;
            }
        };
        let out = sb_buf.as_mut();
        for sy in 0..PANEL_H {
            for sx in 0..PANEL_W {
                let raw = src[(sy * PANEL_W + sx) as usize];
                // Expand RGB565 → 0x00RRGGBB
                let r = ((raw >> 11) & 0x1f) as u32;
                let g = ((raw >> 5) & 0x3f) as u32;
                let b = (raw & 0x1f) as u32;
                // Scale to 8-bit: 5-bit → multiply by 255/31; 6-bit → 255/63
                let r8 = (r * 255) / 31;
                let g8 = (g * 255) / 63;
                let b8 = (b * 255) / 31;
                let pixel = (r8 << 16) | (g8 << 8) | b8;
                // Write 2×2 block into output
                let dy = sy * 2;
                let dx = sx * 2;
                out[(dy * STATUS_WIN_W + dx) as usize] = pixel;
                out[(dy * STATUS_WIN_W + dx + 1) as usize] = pixel;
                out[((dy + 1) * STATUS_WIN_W + dx) as usize] = pixel;
                out[((dy + 1) * STATUS_WIN_W + dx + 1) as usize] = pixel;
            }
        }
        if let Err(e) = sb_buf.present() {
            tracing::warn!("status paint present: {e}");
        }
    }

    fn id(&self) -> WindowId {
        self.window.id()
    }
}

// ── main GL window ────────────────────────────────────────────────────────────

pub struct WinitGlTarget {
    gl: Arc<glow::Context>,
    surface: Surface<WindowSurface>,
    context: glutin::context::PossiblyCurrentContext,
    _window: Window,
    main_window_id: WindowId,
    event_loop: EventLoop<()>,
    size: (u32, u32),
    should_exit: bool,
    key_events: Vec<winit::event::KeyEvent>,
    /// When `Some`, the status window will be created on the first `Resumed`
    /// event (winit 0.30 requires window creation inside the event loop).
    pending_status_buf: Option<Arc<Mutex<Vec<u16>>>>,
    /// Live status preview window; `None` until created or after it's closed.
    status_window: Option<StatusWindow>,
}

impl WinitGlTarget {
    pub fn drain_key_events(&mut self) -> Vec<winit::event::KeyEvent> {
        std::mem::take(&mut self.key_events)
    }

    pub fn new(width: u32, height: u32, title: &str) -> Result<Self> {
        let event_loop =
            EventLoop::new().map_err(|e| Error::Backend(format!("event loop: {e}")))?;

        // winit 0.30: use WindowAttributes instead of deprecated WindowBuilder
        let window_attributes = Window::default_attributes()
            .with_inner_size(PhysicalSize::new(width, height))
            .with_title(title);

        let template = ConfigTemplateBuilder::new().with_alpha_size(8);

        // glutin-winit 0.5: with_window_attributes (not with_window_builder)
        let display_builder = DisplayBuilder::new().with_window_attributes(Some(window_attributes));

        let (window, gl_config) = display_builder
            .build(&event_loop, template, |mut configs| configs.next().unwrap())
            .map_err(|e| Error::Backend(format!("display build: {e}")))?;

        let window = window.ok_or_else(|| Error::Backend("no window from glutin".into()))?;

        // rwh_06: use HasWindowHandle::window_handle() then as_raw()
        let raw = window
            .window_handle()
            .map_err(|e| Error::Backend(format!("window handle: {e}")))?
            .as_raw();

        let context_attrs = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 0))))
            .build(Some(raw));

        let not_current = unsafe {
            gl_config
                .display()
                .create_context(&gl_config, &context_attrs)
                .map_err(|e| Error::Backend(format!("create context: {e}")))?
        };

        let attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(
            raw,
            NonZeroU32::new(width).unwrap(),
            NonZeroU32::new(height).unwrap(),
        );

        let surface = unsafe {
            gl_config
                .display()
                .create_window_surface(&gl_config, &attrs)
                .map_err(|e| Error::Backend(format!("surface: {e}")))?
        };

        let context = not_current
            .make_current(&surface)
            .map_err(|e| Error::Backend(format!("make current: {e}")))?;

        let gl = unsafe {
            glow::Context::from_loader_function_cstr(|s| {
                gl_config.display().get_proc_address(s) as *const _
            })
        };
        let gl = Arc::new(gl);
        assert_gles2_capable(&gl)?;

        let main_window_id = window.id();

        Ok(Self {
            gl,
            surface,
            context,
            _window: window,
            main_window_id,
            event_loop,
            size: (width, height),
            should_exit: false,
            key_events: Vec::new(),
            pending_status_buf: None,
            status_window: None,
        })
    }

    /// Call before the main loop to request a live status preview window.
    /// The window is created lazily on the next winit `Resumed` event.
    pub fn enable_status_window(&mut self, buf: Arc<Mutex<Vec<u16>>>) {
        self.pending_status_buf = Some(buf);
    }

    /// Paint the status preview window if it's open. Call once per frame.
    pub fn paint_status(&mut self) {
        if let Some(sw) = self.status_window.as_mut() {
            sw.paint();
        }
    }
}

impl RenderTarget for WinitGlTarget {
    fn gl(&self) -> Arc<glow::Context> {
        self.gl.clone()
    }

    fn dimensions(&self) -> (u32, u32) {
        self.size
    }

    fn present(&mut self) -> Result<()> {
        self.surface
            .swap_buffers(&self.context)
            .map_err(|e| Error::Backend(format!("swap: {e}")))?;
        Ok(())
    }

    fn pump(&mut self) -> bool {
        use winit::platform::pump_events::EventLoopExtPumpEvents;
        let timeout = Some(std::time::Duration::ZERO);
        // Collect outcomes from the closure since we can't borrow `self.surface`
        // and `self.context` mutably/immutably across the &mut self capture.
        let mut should_exit = self.should_exit;
        let mut new_size: Option<(u32, u32)> = None;
        let mut new_key_events: Vec<winit::event::KeyEvent> = Vec::new();
        let main_window_id = self.main_window_id;
        // Clone the pending buf Arc so it survives across multiple pump calls
        // until Resumed fires and the window is created.
        let pending_status_buf: Option<Arc<Mutex<Vec<u16>>>> = self.pending_status_buf.clone();
        let mut new_status_window: Option<StatusWindow> = self.status_window.take();
        let mut status_window_closed = false;
        let mut status_window_created = false;

        #[allow(deprecated)]
        let _status = self.event_loop.pump_events(timeout, |event, target| {
            target.set_control_flow(ControlFlow::Poll);
            match &event {
                // Create the status window the first time the event loop is
                // ready to service window requests. `Resumed` fires at startup
                // on all winit 0.30 platforms (it's the canonical "safe to
                // create windows" signal).
                Event::Resumed => {
                    if let Some(buf) = pending_status_buf.clone() {
                        if new_status_window.is_none() {
                            match StatusWindow::create(target, buf, &self._window) {
                                Ok(sw) => {
                                    tracing::info!("status preview window opened");
                                    new_status_window = Some(sw);
                                    status_window_created = true;
                                }
                                Err(e) => tracing::warn!("status window create: {e}"),
                            }
                        }
                    }
                }
                Event::WindowEvent { window_id, event } => {
                    if *window_id == main_window_id {
                        match event {
                            WindowEvent::CloseRequested => {
                                should_exit = true;
                                target.exit();
                            }
                            WindowEvent::Resized(size) => {
                                new_size = Some((size.width, size.height));
                            }
                            WindowEvent::KeyboardInput { event, .. } => {
                                new_key_events.push(event.clone());
                            }
                            _ => {}
                        }
                    } else if let Some(sw) = &new_status_window {
                        if *window_id == sw.id() {
                            if let WindowEvent::CloseRequested = event {
                                // Closing the status window does NOT exit the app.
                                status_window_closed = true;
                            }
                        }
                    }
                }
                _ => {}
            }
        });

        // Clear `pending_status_buf` once the window has been successfully created
        // so we don't attempt to re-create it on future `Resumed` events.
        if status_window_created {
            self.pending_status_buf = None;
        }

        // Re-stash the status window unless the user closed it.
        if status_window_closed {
            tracing::info!("status preview window closed");
            self.status_window = None;
        } else {
            self.status_window = new_status_window;
        }

        self.should_exit = should_exit;
        self.key_events.extend(new_key_events);
        if let Some((w, h)) = new_size {
            if let (Some(nz_w), Some(nz_h)) = (NonZeroU32::new(w), NonZeroU32::new(h)) {
                self.surface.resize(&self.context, nz_w, nz_h);
                self.size = (w, h);
            }
        }
        !self.should_exit
    }
}
