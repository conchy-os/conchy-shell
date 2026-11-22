//! The default-application database of XDG: which program opens which type.
//!
//! - [`MimeApps`] is one `mimeapps.list`, edited without disturbing the lines
//!   that are not changed.
//! - [`DesktopIndex`] is the desktop entries those files name, and the types
//!   each one lists.
//! - [`Store`] reads and writes the user's file; [`Layers`] puts it among the
//!   other files the specification stacks and says what a type opens with.

mod document;
mod entries;
mod names;
mod store;

pub use document::{Group, MimeApps};
pub use entries::{DesktopEntry, DesktopIndex, find_desktop_file};
pub use names::describe;
pub use store::{Layers, Source, Store, known_mime_types};
