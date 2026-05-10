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

#[cfg(feature = "desktop")]
pub mod headless;
