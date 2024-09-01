use super::{
    MetadataProvider,
    models
};

/// A meta-MetadataProvider that works by daisy-chaining actual MetadataProviders.
/// Think composite pattern.
/// The key document might be updated as it passes through providers, for example
/// to add a MusicBrainz ID. This should help downstream providers locate metadata
/// more accurately.
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
        let mut current_key: bson::Document = key.clone();
        for provider in self.providers.iter() {
            existing = provider.get_album_meta(current_key.clone(), existing);
            // Update key document with new fields
            if let Some(meta) = &existing {
                if let Some(id) = &meta.mbid {
                    if !current_key.contains_key("mbid") {
                        current_key.insert("mbid", id.to_owned());
                    }
                }
            }
        }
        existing
    }

    fn get_artist_meta(
        self: &Self, key: bson::Document, mut existing: Option<models::ArtistMeta>
    ) -> Option<models::ArtistMeta> {
        let mut current_key: bson::Document = key.clone();
        for provider in self.providers.iter() {
            existing = provider.get_artist_meta(current_key.clone(), existing);
            // Update key document with new fields
            if let Some(meta) = &existing {
                if let Some(id) = &meta.mbid {
                    if !current_key.contains_key("mbid") {
                        current_key.insert("mbid", id.to_owned());
                    }
                }
            }
        }
        existing
    }
}
