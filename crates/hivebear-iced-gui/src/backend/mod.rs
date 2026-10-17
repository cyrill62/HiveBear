//! The backend operations the UI calls. These were the Tauri commands; they
//! are now plain functions over `&AppState`, run off the UI thread via
//! `tasks::Ctx`.

pub mod account;
pub mod benchmark;
pub mod chat;
pub mod config;
pub mod inference;
pub mod mesh;
pub mod profile;
pub mod registry;
pub mod secrets;
pub mod telemetry;
pub mod update;
