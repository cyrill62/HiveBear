//! One module per page. Each has a `Msg`, an `update` that may start
//! backend work through `Env`, and a `view` that only reads.

pub mod account;
pub mod benchmark;
pub mod chat;
pub mod dashboard;
pub mod mesh;
pub mod models;
pub mod settings;
