mod base;
pub mod models;
pub mod lastfm;

pub use base::{MetadataProvider, MetadataResponse, utils};

pub mod prelude {
    pub use super::base::MetadataProvider;
    pub use super::models::{Tagged, HasImage};
}
