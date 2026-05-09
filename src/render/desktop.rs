//! Desktop GL context via winit + glutin. Used for macOS/Linux dev.

#![cfg(feature = "desktop")]

use std::num::NonZeroU32;
use std::sync::Arc;

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
use winit::window::Window;

use crate::error::{Error, Result};

use super::target::{assert_gles2_capable, RenderTarget};

pub struct WinitGlTarget {
    gl: Arc<glow::Context>,
    surface: Surface<WindowSurface>,
    context: glutin::context::PossiblyCurrentContext,
    _window: Window,
    event_loop: EventLoop<()>,
    size: (u32, u32),
    should_exit: bool,
}

impl WinitGlTarget {
    pub fn new(width: u32, height: u32, title: &str) -> Result<Self> {
        let event_loop = EventLoop::new()
            .map_err(|e| Error::Backend(format!("event loop: {e}")))?;

        // winit 0.30: use WindowAttributes instead of deprecated WindowBuilder
        let window_attributes = Window::default_attributes()
            .with_inner_size(PhysicalSize::new(width, height))
            .with_title(title);

        let template = ConfigTemplateBuilder::new().with_alpha_size(8);

        // glutin-winit 0.5: with_window_attributes (not with_window_builder)
        let display_builder = DisplayBuilder::new()
            .with_window_attributes(Some(window_attributes));

        let (window, gl_config) = display_builder
            .build(&event_loop, template, |mut configs| {
                configs.next().unwrap()
            })
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

        Ok(Self {
            gl,
            surface,
            context,
            _window: window,
            event_loop,
            size: (width, height),
            should_exit: false,
        })
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
        #[allow(deprecated)]
        let _status = self.event_loop.pump_events(timeout, |event, target| {
            target.set_control_flow(ControlFlow::Poll);
            if let Event::WindowEvent { event, .. } = event {
                match event {
                    WindowEvent::CloseRequested => {
                        self.should_exit = true;
                        target.exit();
                    }
                    WindowEvent::Resized(new_size) => {
                        self.size = (new_size.width, new_size.height);
                    }
                    _ => {}
                }
            }
        });
        !self.should_exit
    }
}
