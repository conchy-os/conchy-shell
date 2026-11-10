//! Composite shell surfaces - whole UIs rather than widgets.
//!
//! Where [`components`](crate::components) are primitives that assume nothing
//! about their surroundings, a pattern is shaped for one shell surface.
//!
//! Kept out of the crate-root glob: `use ui::*` pulls in `components`, never
//! `patterns`, so a pattern is always spelled `ui::patterns::LauncherFrame`.
//! Patterns own presentation only - state and keybindings stay in the app.

pub mod bar;
pub mod frame;
pub mod launcher;
pub mod osd;
pub mod surface;

pub use bar::{BarChip, BarEdge, BarRhythm, BarSegment, BarShape, BarSurface, ChipGeometry};
pub use frame::ScreenFrame;
pub use launcher::{LauncherFrame, footer_hints};
pub use osd::OsdIndicator;
pub use surface::{PanelSurface, card_radius, surface_padding, surface_radius};
