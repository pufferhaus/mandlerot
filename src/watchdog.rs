//! `/dev/watchdog` ping wrapper. Linux/Pi only. No-op shim on other targets.

#[cfg(target_os = "linux")]
mod linux {
    use std::fs::{File, OpenOptions};
    use std::io::Write;
    use std::path::Path;

    use crate::error::Result;

    pub struct Watchdog {
        f: File,
    }

    impl Watchdog {
        pub fn open(path: impl AsRef<Path>) -> Result<Self> {
            let f = OpenOptions::new()
                .write(true)
                .open(path)
                .map_err(|e| crate::Error::Backend(format!("open watchdog: {e}")))?;
            Ok(Self { f })
        }

        pub fn pet(&mut self) {
            // Writing any byte to /dev/watchdog pets it.
            let _ = self.f.write_all(b"\0");
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod stub {
    use std::path::Path;

    use crate::error::Result;

    pub struct Watchdog;

    impl Watchdog {
        pub fn open(_path: impl AsRef<Path>) -> Result<Self> {
            Ok(Self)
        }
        pub fn pet(&mut self) {}
    }
}

#[cfg(target_os = "linux")]
pub use linux::Watchdog;
#[cfg(not(target_os = "linux"))]
pub use stub::Watchdog;
