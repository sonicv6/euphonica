mod base;
pub mod models;
pub mod lastfm;

pub use base::{MetadataProvider, MetadataResponse};

pub mod prelude {
    pub use super::base::MetadataProvider;
}
