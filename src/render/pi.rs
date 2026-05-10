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
            if info.state() != connector::State::Connected {
                continue;
            }
            // The vc4 driver names the composite output "Composite-1" or
            // similar. Match by interface enum.
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

use gbm::{BufferObjectFlags, Device as GbmDevice, Format as GbmFormat, Surface as GbmSurface};

pub struct PiContext {
    card: PiCard,
    gbm: GbmDevice<PiCard>,
    surface: GbmSurface<()>,
    egl: khronos_egl::DynamicInstance<khronos_egl::EGL1_5>,
    egl_display: khronos_egl::Display,
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
                egl.get_proc_address(s.to_str().unwrap_or("")) as *const _
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

    /// Block until the kernel reports a `PageFlip` event for our crtc, then
    /// the previously-`scanning` buffer/FB is safe to release.
    fn wait_for_flip(&mut self) -> Result<()> {
        loop {
            let events = self
                .ctx
                .card
                .receive_events()
                .map_err(|e| Error::Backend(format!("receive_events: {e}")))?;
            for event in events {
                if let drm::control::Event::PageFlip(pf) = event {
                    if pf.crtc == self.ctx.crtc_handle {
                        // Old `scanning` is no longer being scanned out.
                        if let Some((old_fb, _bo)) = self.scanning.take() {
                            let _ = self.ctx.card.destroy_framebuffer(old_fb);
                        }
                        // Promote pending → scanning.
                        if let Some(pending) = self.pending.take() {
                            self.scanning = Some(pending);
                        }
                        return Ok(());
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

        if !self.first_flip_done {
            // First frame: full mode-set. After this returns the buffer is
            // already scanning out, so move it directly to `scanning`.
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
            self.scanning = Some((fb, bo));
            self.first_flip_done = true;
            return Ok(());
        }

        // If a previous page-flip is still pending, we must wait for its
        // completion event before queueing another (kernel only allows one
        // pending flip per crtc).
        // FIXME(plan-3-deploy): verify this single-pending invariant on hardware.
        if self.pending.is_some() {
            self.wait_for_flip()?;
        }

        self.ctx
            .card
            .page_flip(
                self.ctx.crtc_handle,
                fb,
                drm::control::PageFlipFlags::EVENT,
                None,
            )
            .map_err(|e| Error::Backend(format!("page_flip: {e}")))?;
        self.pending = Some((fb, bo));
        Ok(())
    }

    fn pump(&mut self) -> bool {
        // Pi has no event loop; SIGINT/SIGTERM handled by systemd.
        !self.should_exit
    }
}

impl Drop for PiTarget {
    fn drop(&mut self) {
        // Best-effort: drain any pending flip event so we don't destroy
        // a buffer the kernel is still scanning out.
        if self.pending.is_some() {
            let _ = self.wait_for_flip();
        }
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
