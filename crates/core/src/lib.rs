//! Backend-free calculator core, shared by the native TUI and the web build.
//!
//! Nothing here depends on crossterm, arboard, or ratzilla: input arrives as
//! [`action::Action`]s (or raw chars/labels resolved through it), and rendering
//! draws into any Ratatui [`ratatui::Frame`]. The event loop, clipboard, and
//! terminal lifecycle belong to each entry point.

pub mod action;
pub mod app;
pub mod eval;
pub mod layout;
pub mod ui;
pub mod ui_state;
