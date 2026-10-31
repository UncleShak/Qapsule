//! Qapsule's built-in data sources. Each provider only calls the engine's
//! `provider_upsert` and `provider_remove`; it never touches the UI.

pub mod calendar;
pub mod media;
pub mod notifications;
pub mod weather;
