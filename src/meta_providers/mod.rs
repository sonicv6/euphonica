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
    pub use super::base::MetadataProvider;
    pub use super::models::{Tagged, HasImage, Merge};
}
