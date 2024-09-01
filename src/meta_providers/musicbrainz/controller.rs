extern crate bson;

use musicbrainz_rs::{
    prelude::*,
    entity::{
        artist::*,
        release::*
    }
};

use crate::utils::settings_manager;

use super::super::{
    models, prelude::*, MetadataProvider
};
use super::models::*;

pub struct MusicBrainzWrapper {}

impl MetadataProvider for MusicBrainzWrapper {
    fn new() -> Self {
        Self {}
    }

    /// Schedule getting album metadata from MusicBrainz.
    /// A signal will be emitted to notify the caller when the result arrives.
    fn get_album_meta(
        &self,
        key: bson::Document,
        existing: Option<models::AlbumMeta>
    ) -> Option<models::AlbumMeta> {
        if let Some(mbid) = key.get("mbid") {
            println!("[MusicBrainz] Fetching release by MBID: {}", &mbid);
            let res = Release::fetch()
                .id(mbid.as_str().unwrap())
                .execute();
            if let Ok(release) = res {
                let new: models::AlbumMeta = release.into();
                println!("{:?}", &new);
                // If there is existing data, merge new data to it
                if let Some(old) = existing {
                    return Some(old.merge(new));
                }
                return Some(new);
            }
            else {
                println!("[MusicBrainz] Could not fetch album metadata: {:?}", res.err());
                return None;
            }
        }
        // Else there must be an artist tag before we can search reliably
        else if let (Some(title), Some(artist)) = (key.get("name"), key.get("artist")) {
            // Ensure linkages match those on MusicBrainz.
            // TODO: use multiple ORed artist clauses instead.
            println!("[MusicBrainz] Searching release with title = {title} and artist = {artist}");
            let res = Release::search(
                ReleaseSearchQuery::query_builder()
                    .release(title.as_str().unwrap())
                    .artist(artist.as_str().unwrap())
                    .build()
            ).execute();

            if let Ok(found) = res {
                if let Some(first) = found.entities.into_iter().nth(0) {
                    let new: models::AlbumMeta = first.into();
                    // If there is existing data, merge new data to it
                    if let Some(old) = existing {
                        return Some(old.merge(new));
                    }
                    return Some(new);
                }
                else {
                    println!("[MusicBrainz] No release found for artist & album title");
                    return None;
                }
            }
            else {
                println!("[MusicBrainz] Could not fetch album metadata: {:?}", res.err());
                return None;
            }
        }
        else {
            println!("[MusicBrainz] Either MBID or BOTH album name & artist must be provided");
            return None;
        }
    }

    /// Schedule getting artist metadata from MusicBrainz.
    /// A signal will be emitted to notify the caller when the result arrives.
    /// Since images have varying aspect ratios, we will use a simple entropy-based cropping
    /// algorithm.
    fn get_artist_meta(
        &self,
        key: bson::Document,
        existing: std::option::Option<models::ArtistMeta>
    ) -> Option<models::ArtistMeta> {
        if let Some(mbid) = key.get("mbid") {
            println!("[MusicBrainz] Fetching artist by MBID: {}", &mbid);
            let res = Artist::fetch()
                .id(mbid.as_str().unwrap())
                .execute();
            if let Ok(artist) = res {
                let new: models::ArtistMeta = artist.into();
                println!("{:?}", &new);
                // If there is existing data, merge new data to it
                if let Some(old) = existing {
                    return Some(old.merge(new));
                }
                return Some(new);
            }
            else {
                println!("[MusicBrainz] Could not fetch artist metadata: {:?}", res.err());
                return None;
            }
        }
        // If MBID is not available we'll need to search solely by artist name.
        // TODO: add some more clues, such as a song or album name.
        else if let Some(name) = key.get("name") {
            println!("[MusicBrainz] Fetching artist with name = {}", &name);
            let res = Artist::search(
                ArtistSearchQuery::query_builder()
                    .artist(name.as_str().unwrap())
                    .build()
            ).execute();

            if let Ok(found) = res {
                if let Some(first) = found.entities.into_iter().nth(0) {
                    let new: models::ArtistMeta = first.into();
                    println!("{:?}", &new);
                    // If there is existing data, merge new data to it
                    if let Some(old) = existing {
                        return Some(old.merge(new));
                    }
                    return Some(new);
                }
                else {
                    println!("[MusicBrainz] No artist metadata found for {name}");
                    return None;
                }
            }
            else {
                println!("[MusicBrainz] Could not fetch artist metadata: {:?}", res.err());
                return None;
            }
        }
        else {
            println!("[MusicBrainz] Key document is empty, will not fetch artist metadata");
            return None;
        }
    }
}
