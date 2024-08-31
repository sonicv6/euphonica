extern crate bson;

use reqwest::{
    blocking::{Client, Response},
    header::USER_AGENT
};
use gtk::prelude::*;

use crate::utils::settings_manager;

use super::super::{
    models, prelude::*, MetadataProvider
};
use super::models::{LastfmAlbumResponse, LastfmArtistResponse};

pub const API_ROOT: &str = "http://ws.audioscrobbler.com/2.0";

enum Task {
    AlbumMeta(String, bson::Document), // folder_uri and key doc
    ArtistMeta(bson::Document)
}

pub struct LastfmWrapper {
    client: Client
}

impl LastfmWrapper {
    fn get_lastfm(
        &self,
        method: &str,
        params: &[(&str, String)]
    ) -> Option<Response> {
        let settings = settings_manager().child("client");
        let key = settings.string("lastfm-api-key").to_string();
        let agent = settings.string("lastfm-user-agent").to_string();
        // Return None if there is no API key specified.
        if !key.is_empty() && !agent.is_empty() {
            println!("Last.fm: calling `{}` with query {:?}", method, params);
            let resp = self.client
                .get(API_ROOT)
                .query(&[
                    ("format", "json"),
                    ("method", method),
                    ("api_key", key.as_ref())
                ])
                .query(params)
                .header(USER_AGENT, agent)
                .send();
            if let Ok(res) = resp {
                return Some(res);
            }
            return None;
        }
        None
    }
}

impl MetadataProvider for LastfmWrapper {
    fn new() -> Self {
        Self {
            client: Client::new()
        }
    }

    /// Schedule getting album metadata from Last.fm.
    /// A signal will be emitted to notify the caller when the result arrives.
    fn get_album_meta(
        &self,
        key: bson::Document,
        existing: Option<models::AlbumMeta>
    ) -> Option<models::AlbumMeta> {
        // Will panic if key document is not a simple map of String to String
        let params: Vec<(&str, String)> = key.iter().map(
            |kv: (&String, &bson::Bson)| {
                (kv.0.as_ref(), kv.1.as_str().unwrap().to_owned())
            }
        ).collect();

        if let Some(resp) = self.get_lastfm(
            "album.getinfo",
            &params
        ) {
            // TODO: Get summary
            match resp.status() {
                reqwest::StatusCode::OK => {
                    match resp.json::<LastfmAlbumResponse>() {
                        Ok(parsed) => {
                            // Some preprocessing is needed.
                            // Might have to put the mbid back in, as Last.fm won't return
                            // it if we queried using it in the first place.
                            let mut new: models::AlbumMeta = parsed.album.into();
                            if let Some(id) = key.get("mbid") {
                                new.mbid = Some(id.as_str().unwrap().to_owned());
                            }
                            // Override album & artist names in case the returned values
                            // are slightly different (casing, apostrophes, etc.), else
                            // we won't be able to query it back using our own tags.
                            if let Some(artist) = key.get("artist") {
                                new.artist = artist.as_str().unwrap().to_owned();
                            }
                            if let Some(name) = key.get("album") {
                                new.name = name.as_str().unwrap().to_owned();
                            }
                            // If there is existing data, merge new data to it
                            if let Some(old) = existing {
                                Some(old.merge(new))
                            }
                            else {
                                Some(new)
                            }
                        }
                        Err(err) => {
                            println!("[Last.fm] get_album_meta: {}", err);
                            None
                        }
                    }
                }
                other => {
                    println!("[Last.fm] get_album_meta: failed with status {:?}", other);
                    None
                }
            }
        }
        else {
            None
        }
    }

    /// Schedule getting artist metadata from Last.fm.
    /// A signal will be emitted to notify the caller when the result arrives.
    fn get_artist_meta(
        &self,
        key: bson::Document,
        existing: std::option::Option<models::ArtistMeta>
    ) -> Option<models::ArtistMeta> {
        // Will panic if key document is not a simple map of String to String
        let params: Vec<(&str, String)> = key.iter().map(
            |kv: (&String, &bson::Bson)| {
                // Last.fm wants "artist" in query param but will return "name".
                // Our bson key follows the returned result schema so it'll have to be renamed here.
                (if kv.0 == "name" { "artist" } else { kv.0.as_ref() }, kv.1.as_str().unwrap().to_owned())
            }
        ).collect();
        if let Some(resp) = self.get_lastfm(
            "artist.getinfo",
            &params
        ) {
            // TODO: Get summary
            match resp.status() {
                reqwest::StatusCode::OK => {
                    match resp.json::<LastfmArtistResponse>() {
                        Ok(parsed) => {
                            // Some preprocessing is needed.
                            // Might have to put the mbid back in, as Last.fm won't return
                            // it if we queried using it in the first place.
                            let mut new: models::ArtistMeta = parsed.artist.into();
                            if let Some(id) = key.get("mbid") {
                                new.mbid = Some(id.as_str().unwrap().to_owned());
                            }
                            // Override artist name in case the returned values
                            // are slightly different (casing, apostrophes, etc.), else
                            // we won't be able to query it back using our own tags.
                            if let Some(name) = key.get("artist") {
                                new.name = name.as_str().unwrap().to_owned();
                            }
                            if let Some(old) = existing {
                                Some(old.merge(new))
                            }
                            else {
                                Some(new)
                            }
                        },
                        Err(err) => {
                            println!("[Last.fm] get_artist_meta: {}", err);
                            None
                        }
                    }
                }
                other => {
                    println!("[Last.fm] get_artist_meta: failed with status {:?}", other);
                    None
                }
            }
        }
        else {
            None
        }
    }
}
