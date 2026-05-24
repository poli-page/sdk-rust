//! Wire-shape types exposed by the public surface.

pub mod document;
pub mod input;
pub mod page_format;
pub mod preview;
pub mod thumbnail;

pub use document::{DocumentDescriptor, DocumentPreviewResult};
pub use input::{InlineModeInput, MetadataValue, ProjectModeInput, RenderInput, RenderMetadata};
pub use page_format::{Orientation, PageFormat};
pub use preview::{Environment, PreviewResult};
pub use thumbnail::{Thumbnail, ThumbnailFormat, ThumbnailOptions};
