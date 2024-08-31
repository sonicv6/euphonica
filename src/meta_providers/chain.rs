use super::{
    MetadataProvider,
    models
};

/// A meta-MetadataProvider that works by daisy-chaining actual MetadataProviders.
/// Think composite pattern.

pub struct MetadataChain {
    pub providers: Vec<Box<dyn MetadataProvider>>
}

impl MetadataProvider for MetadataChain {
    fn new() -> Self where Self: Sized {
        Self {
            providers: Vec::new()
        }
    }

    fn get_album_meta(
        self: &Self, key: bson::Document, mut existing: Option<models::AlbumMeta>
    ) -> Option<models::AlbumMeta> {
        for provider in self.providers.iter() {
            existing = provider.get_album_meta(key.clone(), existing);
        }
        existing
    }

    fn get_artist_meta(
        self: &Self, key: bson::Document, mut existing: Option<models::ArtistMeta>
    ) -> Option<models::ArtistMeta> {
        for provider in self.providers.iter() {
            existing = provider.get_artist_meta(key.clone(), existing);
        }
        existing
    }
}
