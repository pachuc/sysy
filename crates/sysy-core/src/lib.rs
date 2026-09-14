//! The sysy design model, its file format, and validation.
//!
//! See `docs/DESIGN.md` for the design this crate implements.

mod file;
mod model;
mod validation;

pub use file::{Error, VERSION, load, save};
pub use model::{
    Container, Design, Edge, EdgeKind, Layout, LayoutEntry, Node, NodeKind, Note, Size,
};
pub use validation::{Problem, validate};
