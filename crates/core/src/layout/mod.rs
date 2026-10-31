//! Library layout: templates, grouping files into item folders, proposing a
//! template for existing folders, and placing new files by write policy.

mod group;
mod place;
mod propose;
mod template;

pub use group::{Group, Hints, corrected_ids, group};
pub use place::{Incoming, Placement, WritePolicy, place};
pub use propose::{Proposal, propose};
pub use template::{
    DEFAULT_MOVIES, DEFAULT_TV, LibraryKind, Template, TemplateError, TemplatePiece, TemplateRole,
};
