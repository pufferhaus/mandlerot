//! mandleROT — generative video VJ tool.

pub mod error;

pub use error::{Error, Result};

pub mod config;

pub mod scene;

pub mod render;

pub mod state;

pub mod action;

pub mod apply;

pub mod hot_reload;

pub mod input;

pub mod audio;

pub mod preset;

pub mod tap_tempo;

pub mod status;

pub mod overlay;

pub mod ui;

pub mod supervisor;

pub mod watchdog;

pub mod platform;

pub mod video;

#[cfg(feature = "desktop")]
pub mod headless;
