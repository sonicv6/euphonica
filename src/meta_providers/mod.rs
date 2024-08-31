mod base;
mod chain;
pub mod models;
pub mod lastfm;

pub use chain::MetadataChain;
pub use base::{MetadataProvider, Metadata, utils};

pub mod prelude {
    pub use super::base::MetadataProvider;
    pub use super::models::{Tagged, HasImage, Merge};
}
