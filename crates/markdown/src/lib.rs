//! Markdown rendering and source editing for PossessApp.
//!
//! Deliberately free of any browser dependency: this is the logic the preview
//! and the editor share, and keeping it platform-independent means it can be
//! tested with `cargo test` rather than only by driving a browser.

pub mod assets;
pub mod render;
pub mod source;
pub mod wikilinks;

pub use render::{render, RenderContext};
