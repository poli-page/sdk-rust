//! Wire-shape types exposed by the public surface.

/// An enum of the string values the SDK knows, plus `Unknown(String)` for
/// any other value, kept verbatim: a value the API adds round-trips through
/// deserialize and serialize unchanged (spec §10).
macro_rules! open_string_enum {
    (
        $(#[$meta:meta])*
        pub enum $name:ident {
            $( $(#[$vmeta:meta])* $variant:ident = $wire:literal, )+
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub enum $name {
            $( $(#[$vmeta])* $variant, )+
            /// A value this SDK version does not know, kept verbatim so it
            /// is passed through unchanged. Build values from a string with
            /// `From<&str>`, which maps the known ones to their variant.
            Unknown(String),
        }

        impl $name {
            /// The wire value.
            #[must_use]
            pub fn as_str(&self) -> &str {
                match self {
                    $( Self::$variant => $wire, )+
                    Self::Unknown(value) => value,
                }
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                match value {
                    $( $wire => Self::$variant, )+
                    other => Self::Unknown(other.to_owned()),
                }
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                match value.as_str() {
                    $( $wire => Self::$variant, )+
                    _ => Self::Unknown(value),
                }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl serde::Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                String::deserialize(deserializer).map(Self::from)
            }
        }
    };
}

pub mod document;
pub mod input;
pub mod page_format;
pub mod thumbnail;

pub use document::{DocumentDescriptor, DocumentPreviewResult};
pub use input::{MetadataValue, RenderInput, RenderMetadata};
pub use page_format::{Orientation, PageFormat};
pub use thumbnail::{Thumbnail, ThumbnailFormat, ThumbnailOptions};
