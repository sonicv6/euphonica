mod base;
mod chain;
pub mod models;
pub mod lastfm;
pub mod musicbrainz;

pub use chain::{
    MetadataChain,
    get_provider_with_priority
};
pub use base::{MetadataProvider, Metadata, utils};

pub mod prelude {
    pub use super::base::{MetadataProvider, sleep_after_request};
    pub use super::models::Merge;
}
