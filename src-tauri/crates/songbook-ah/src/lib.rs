//! Allen & Heath driver: SQ, dLive, Avantis (and the Qu / CQ descriptors).
//!
//! Show files come in through [`import_path`] / [`import_bytes`]; live desks
//! through [`live::Device`]. Everything lands in `songbook_model::Show`.

pub mod dlive;
pub mod import;
pub mod live;
pub mod midi;
pub mod sq;

pub use midi::PORT;
