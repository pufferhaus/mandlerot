#![cfg(all(feature = "pi", target_os = "linux"))]

//! Raspberry Pi composite-output backend via DRM/KMS + GBM + EGL.
//!
//! Tested on Pi 3 B+ with `enable_tvout=1` and `sdtv_mode=0` in
//! `/boot/firmware/config.txt`. Same code path on Pi 4/5 with
//! appropriate composite hardware adapter.

use std::os::fd::AsFd;
use std::sync::Arc;

use drm::control::{connector, framebuffer, Device as ControlDevice};
use drm::Device as BasicDevice;

use crate::error::{Error, Result};

pub struct PiCard {
    file: std::fs::File,
}

impl AsFd for PiCard {
    fn as_fd(&self) -> std::os::fd::BorrowedFd<'_> {
        self.file.as_fd()
    }
}

impl BasicDevice for PiCard {}
impl ControlDevice for PiCard {}

impl PiCard {
    pub fn open(path: &str) -> Result<Self> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(|e| Error::Backend(format!("open {path}: {e}")))?;
        Ok(Self { file })
    }

    pub fn open_default() -> Result<Self> {
        // Try card0 first; fall back to card1 if needed.
        for path in ["/dev/dri/card0", "/dev/dri/card1"] {
            if let Ok(c) = Self::open(path) {
                return Ok(c);
            }
        }
        Err(Error::Backend("no DRM device found".into()))
    }

    /// Find the first connected composite (TV) connector.
    pub fn find_composite_connector(&self) -> Result<connector::Info> {
        let resources = self
            .resource_handles()
            .map_err(|e| Error::Backend(format!("resources: {e}")))?;
        for handle in resources.connectors() {
            let info = self
                .get_connector(*handle, false)
                .map_err(|e| Error::Backend(format!("connector: {e}")))?;
            // Composite/TV/SVideo are analog with no hot-plug detect, so the
            // KMS state is always "Unknown" — accept that as good. Reject
            // explicit Disconnected.
            if info.state() == connector::State::Disconnected {
                continue;
            }
            use connector::Interface::*;
            if matches!(info.interface(), Composite | TV | SVideo) {
                return Ok(info);
            }
        }
        Err(Error::Backend(
            "no connected composite/TV connector found".into(),
        ))
    }
}

use gbm::{AsRaw, BufferObjectFlags, Device as GbmDevice, Format as GbmFormat, Surface as GbmSurface};

pub struct PiContext {
    card: PiCard,
    // RAII: must outlive surface/egl below; dropping closes the gbm+EGL handles.
    #[allow(dead_code)]
    gbm: GbmDevice<PiCard>,
    surface: GbmSurface<()>,
    egl: khronos_egl::DynamicInstance<khronos_egl::EGL1_5>,
    egl_display: khronos_egl::Display,
    #[allow(dead_code)]
    egl_context: khronos_egl::Context,
    egl_surface: khronos_egl::Surface,
    crtc_handle: drm::control::crtc::Handle,
    connector_handle: drm::control::connector::Handle,
    mode: drm::control::Mode,
    width: u32,
    height: u32,
    gl: Arc<glow::Context>,
}

impl PiContext {
    pub fn create(width_hint: u32, height_hint: u32) -> Result<Self> {
        let card = PiCard::open_default()?;
        let conn = card.find_composite_connector()?;
        let mode = conn
            .modes()
            .iter()
            .find(|m| m.size() == (width_hint as u16, height_hint as u16))
            .or_else(|| conn.modes().first())
            .copied()
            .ok_or_else(|| Error::Backend("no display modes".into()))?;
        let (width, height) = (mode.size().0 as u32, mode.size().1 as u32);

        let encoder_handle = conn
            .current_encoder()
            .ok_or_else(|| Error::Backend("connector has no encoder".into()))?;
        let enc = card
            .get_encoder(encoder_handle)
            .map_err(|e| Error::Backend(format!("encoder: {e}")))?;
        let crtc_handle = enc
            .crtc()
            .ok_or_else(|| Error::Backend("encoder has no CRTC".into()))?;

        let card_for_gbm = PiCard {
            file: card
                .file
                .try_clone()
                .map_err(|e| Error::Backend(format!("dup: {e}")))?,
        };
        let gbm =
            GbmDevice::new(card_for_gbm).map_err(|e| Error::Backend(format!("gbm device: {e}")))?;
        let surface = gbm
            .create_surface::<()>(
                width,
                height,
                GbmFormat::Argb8888,
                BufferObjectFlags::SCANOUT | BufferObjectFlags::RENDERING,
            )
            .map_err(|e| Error::Backend(format!("gbm surface: {e}")))?;

        let egl = unsafe {
            khronos_egl::DynamicInstance::<khronos_egl::EGL1_5>::load_required()
                .map_err(|e| Error::Backend(format!("load EGL: {e}")))?
        };
        let egl_display = unsafe {
            egl.get_display(gbm.as_raw_mut() as *mut _)
                .ok_or_else(|| Error::Backend("get EGL display".into()))?
        };
        egl.initialize(egl_display)
            .map_err(|e| Error::Backend(format!("egl init: {e}")))?;

        let attribs = [
            khronos_egl::SURFACE_TYPE,
            khronos_egl::WINDOW_BIT,
            khronos_egl::RED_SIZE,
            8,
            khronos_egl::GREEN_SIZE,
            8,
            khronos_egl::BLUE_SIZE,
            8,
            khronos_egl::ALPHA_SIZE,
            8,
            khronos_egl::RENDERABLE_TYPE,
            khronos_egl::OPENGL_ES2_BIT,
            khronos_egl::NONE,
        ];
        let config = egl
            .choose_first_config(egl_display, &attribs)
            .map_err(|e| Error::Backend(format!("choose config: {e}")))?
            .ok_or_else(|| Error::Backend("no matching EGL config".into()))?;

        let ctx_attribs = [khronos_egl::CONTEXT_CLIENT_VERSION, 2, khronos_egl::NONE];
        egl.bind_api(khronos_egl::OPENGL_ES_API)
            .map_err(|e| Error::Backend(format!("bind api: {e}")))?;
        let egl_context = egl
            .create_context(egl_display, config, None, &ctx_attribs)
            .map_err(|e| Error::Backend(format!("create ctx: {e}")))?;
        let egl_surface = unsafe {
            egl.create_window_surface(egl_display, config, surface.as_raw_mut() as *mut _, None)
                .map_err(|e| Error::Backend(format!("create surface: {e}")))?
        };
        egl.make_current(
            egl_display,
            Some(egl_surface),
            Some(egl_surface),
            Some(egl_context),
        )
        .map_err(|e| Error::Backend(format!("make current: {e}")))?;

        let gl = unsafe {
            glow::Context::from_loader_function_cstr(|s| {
                egl.get_proc_address(s.to_str().unwrap_or(""))
                    .map(|f| f as *const _)
                    .unwrap_or(std::ptr::null())
            })
        };
        let gl = Arc::new(gl);
        super::target::assert_gles2_capable(&gl)?;

        Ok(Self {
            card,
            gbm,
            surface,
            egl,
            egl_display,
            egl_context,
            egl_surface,
            crtc_handle,
            connector_handle: conn.handle(),
            mode,
            width,
            height,
            gl,
        })
    }
}

/// Wrapper that adapts `gbm::BufferObject<()>` to `drm::buffer::PlanarBuffer`
/// without advertising a modifier. The underlying `BufferObject` already
/// implements `PlanarBuffer` via the `drm-support` feature, but its
/// `modifier()` always returns `Some(_)`. `add_planar_framebuffer` then
/// asserts that the caller passes `FbCmd2Flags::MODIFIERS` — which we don't
/// want for plain ARGB8888 SCANOUT buffers. By returning `None` here we
/// satisfy the assertion with `FbCmd2Flags::empty()`.
struct GbmFb<'a> {
    bo: &'a gbm::BufferObject<()>,
}

impl<'a> drm::buffer::PlanarBuffer for GbmFb<'a> {
    fn size(&self) -> (u32, u32) {
        (self.bo.width().unwrap_or(0), self.bo.height().unwrap_or(0))
    }
    fn format(&self) -> drm::buffer::DrmFourcc {
        drm::buffer::DrmFourcc::Argb8888
    }
    fn modifier(&self) -> Option<drm::buffer::DrmModifier> {
        None
    }
    fn pitches(&self) -> [u32; 4] {
        [self.bo.stride().unwrap_or(0), 0, 0, 0]
    }
    fn handles(&self) -> [Option<drm::buffer::Handle>; 4] {
        // BufferObject's own DrmBuffer impl converts the gbm union handle
        // to a drm::buffer::Handle. Reuse that.
        [
            Some(<gbm::BufferObject<()> as drm::buffer::Buffer>::handle(
                self.bo,
            )),
            None,
            None,
            None,
        ]
    }
    fn offsets(&self) -> [u32; 4] {
        [0; 4]
    }
}

pub struct PiTarget {
    ctx: PiContext,
    /// Queued for next vblank (committed via page_flip but not yet scanning out).
    pending: Option<(framebuffer::Handle, gbm::BufferObject<()>)>,
    /// Currently scanning out (returned to us by the most recent flip event).
    scanning: Option<(framebuffer::Handle, gbm::BufferObject<()>)>,
    first_flip_done: bool,
    should_exit: bool,
}

impl PiTarget {
    pub fn new(width_hint: u32, height_hint: u32) -> Result<Self> {
        let ctx = PiContext::create(width_hint, height_hint)?;
        Ok(Self {
            ctx,
            pending: None,
            scanning: None,
            first_flip_done: false,
            should_exit: false,
        })
    }

    fn add_fb_for_bo(&self, bo: &gbm::BufferObject<()>) -> Result<framebuffer::Handle> {
        self.ctx
            .card
            .add_planar_framebuffer(&GbmFb { bo }, drm::control::FbCmd2Flags::empty())
            .map_err(|e| Error::Backend(format!("add_planar_framebuffer: {e}")))
    }

    /// Non-blocking drain of any pending flip-completion events. Uses
    /// poll(2) with a zero timeout so we never block on absent vblanks
    /// (e.g. composite with no TV attached).
    fn drain_flip_events(&mut self) {
        use std::os::fd::AsRawFd;
        let fd = self.ctx.card.as_fd().as_raw_fd();
        let mut pfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: pfd is a valid pollfd with a valid fd; timeout=0 = non-blocking.
        let n = unsafe { libc::poll(&mut pfd, 1, 0) };
        if n <= 0 {
            return;
        }
        let Ok(events) = self.ctx.card.receive_events() else {
            return;
        };
        for event in events {
            if let drm::control::Event::PageFlip(pf) = event {
                if pf.crtc == self.ctx.crtc_handle {
                    if let Some((old_fb, _bo)) = self.scanning.take() {
                        let _ = self.ctx.card.destroy_framebuffer(old_fb);
                    }
                    if let Some(pending) = self.pending.take() {
                        self.scanning = Some(pending);
                    }
                }
            }
        }
    }
}

impl super::target::RenderTarget for PiTarget {
    fn gl(&self) -> Arc<glow::Context> {
        self.ctx.gl.clone()
    }

    fn dimensions(&self) -> (u32, u32) {
        (self.ctx.width, self.ctx.height)
    }

    fn present(&mut self) -> Result<()> {
        self.ctx
            .egl
            .swap_buffers(self.ctx.egl_display, self.ctx.egl_surface)
            .map_err(|e| Error::Backend(format!("swap: {e}")))?;
        // SAFETY: lock_front_buffer requires the EGL context to have just
        // rendered to the surface; swap_buffers above satisfies that.
        let bo = unsafe {
            self.ctx
                .surface
                .lock_front_buffer()
                .map_err(|e| Error::Backend(format!("lock_front_buffer: {e}")))?
        };
        let fb = self.add_fb_for_bo(&bo)?;

        // Every frame is a synchronous mode-set. The original implementation
        // used `page_flip(.., EVENT)` with non-blocking event drain, but on
        // the Pi 3B+ VEC composite encoder the pageflip events fired
        // unreliably — eventually we'd `destroy_framebuffer` on the fb the
        // kernel still had attached to the primary plane, the plane would
        // get detached (`crtc=null` in `/sys/kernel/debug/dri/0/state`),
        // and TV output silently went black even though the app kept
        // rendering at 30 fps. `set_crtc` blocks until the next vblank and
        // the kernel handles plane attach/detach atomically, so the old
        // fb is guaranteed to be off the plane by the time we destroy it
        // below. Composite is interlaced 30 fps, so the per-frame block
        // costs us nothing the display can show.
        self.ctx
            .card
            .set_crtc(
                self.ctx.crtc_handle,
                Some(fb),
                (0, 0),
                &[self.ctx.connector_handle],
                Some(self.ctx.mode),
            )
            .map_err(|e| Error::Backend(format!("set_crtc: {e}")))?;

        // After set_crtc returns the new fb is the active scanout. The
        // previously-scanning fb is no longer attached to the plane, so we
        // can free it. The `pending` slot is unused on this code path but
        // we drain it too in case a prior implementation left something
        // behind across a hot-reload.
        if let Some((old_fb, _bo)) = self.scanning.replace((fb, bo)) {
            let _ = self.ctx.card.destroy_framebuffer(old_fb);
        }
        if let Some((stale_fb, _bo)) = self.pending.take() {
            let _ = self.ctx.card.destroy_framebuffer(stale_fb);
        }
        self.first_flip_done = true;
        Ok(())
    }

    fn pump(&mut self) -> bool {
        // Pi has no event loop; SIGINT/SIGTERM handled by systemd.
        !self.should_exit
    }
}

impl Drop for PiTarget {
    fn drop(&mut self) {
        // Best-effort: drain any completed flip events without blocking.
        self.drain_flip_events();
        if let Some((fb, _)) = self.scanning.take() {
            let _ = self.ctx.card.destroy_framebuffer(fb);
        }
        if let Some((fb, _)) = self.pending.take() {
            let _ = self.ctx.card.destroy_framebuffer(fb);
        }
    }
}

#[cfg(test)]
mod tests {
    // Pi-only — no host tests. See deploy integration in Plan 1 Task 24.
}
