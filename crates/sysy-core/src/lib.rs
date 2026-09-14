//! The sysy design model, its file format, and validation.
//!
//! See `docs/DESIGN.md` for the design this crate implements.

mod edit;
mod file;
mod model;
mod validation;

pub use edit::{ContainerUpdate, EdgeUpdate, NodeUpdate, NoteUpdate, OptionalUpdate};
pub use file::{Error, VERSION, create, load, save};
pub use model::{
    Container, Design, Edge, EdgeKind, Layout, LayoutEntry, Node, NodeKind, Note, Size,
};
pub use validation::{Problem, validate};
