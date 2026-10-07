//! minifb does not surface X11 Expose events. Listen on a separate connection
//! so an uncovered window can repaint without uploading unchanged frames on a
//! timer. This connection and all its Xlib calls stay on the window thread.
#[cfg(target_os = "linux")]
pub struct Repaint {
    x11: Option<(
        x11_dl::xlib::Xlib,
        *mut x11_dl::xlib::Display,
        std::os::raw::c_ulong,
    )>,
}
#[cfg(target_os = "linux")]
impl Repaint {
    pub fn new(window: &minifb::Window) -> eyre::Result<Self> {
        use minifb::HasWindowHandle;
        let mut result = Self { x11: None };
        let handle = window
            .window_handle()
            .map_err(|e| eyre::eyre!("window handle: {e}"))?;
        if let raw_window_handle::RawWindowHandle::Xlib(handle) = handle.as_raw() {
            let lib = x11_dl::xlib::Xlib::open()?;
            // SAFETY: XOpenDisplay creates a connection owned exclusively by
            // this thread. The window handle belongs to the same X server.
            let display = unsafe { (lib.XOpenDisplay)(std::ptr::null()) };
            eyre::ensure!(!display.is_null(), "cannot open X11 repaint connection");
            // SAFETY: display is our live connection and handle.window comes
            // from the still-live minifb window. ExposureMask may be selected
            // by multiple clients; this does not replace minifb's event mask.
            unsafe {
                (lib.XSelectInput)(display, handle.window, x11_dl::xlib::ExposureMask);
                (lib.XFlush)(display);
            }
            result.x11 = Some((lib, display, handle.window));
        }
        Ok(result)
    }
    pub fn requested(&self) -> bool {
        let Some((lib, display, window)) = &self.x11 else {
            return false;
        };
        let mut event = std::mem::MaybeUninit::uninit();
        let mut repaint = false;
        // SAFETY: display is live and thread-local. XCheckWindowEvent writes
        // to event only when it succeeds; we need only whether an event exists.
        while unsafe {
            (lib.XCheckWindowEvent)(
                *display,
                *window,
                x11_dl::xlib::ExposureMask,
                event.as_mut_ptr(),
            )
        } != 0
        {
            repaint = true;
        }
        repaint
    }
}
#[cfg(target_os = "linux")]
impl Drop for Repaint {
    fn drop(&mut self) {
        if let Some((lib, display, _)) = &self.x11 {
            // SAFETY: this is the single close of our owned connection. lib
            // remains loaded through the call, and minifb owns another display.
            unsafe {
                (lib.XCloseDisplay)(*display);
            }
        }
    }
}

#[cfg(not(target_os = "linux"))]
pub struct Repaint;
#[cfg(not(target_os = "linux"))]
impl Repaint {
    pub fn new(_: &minifb::Window) -> eyre::Result<Self> {
        Ok(Self)
    }
    pub fn requested(&self) -> bool {
        false
    }
}
