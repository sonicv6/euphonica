use std::cell::Cell;
use reqwest::{
    Client,
    Response,
    Error
};
use gtk::{
    gio::Settings,
    prelude::*
};

use crate::{
    utils::settings_manager,
    common::AlbumInfo
};

pub const API_ROOT: &str = "http://ws.audioscrobbler.com/2.0";

pub struct LastfmWrapper {
    client: Client,
    settings: Settings
}

impl LastfmWrapper {
    pub fn new() -> Self {
        let app_settings = settings_manager();
        Self {
            client: reqwest::Client::new(),
            settings: app_settings.child("client")
        }
    }

    fn get_api_key(&self) -> Option<String> {
        let maybe_key = self.settings.string("lastfm-api-key");
        if !maybe_key.is_empty() {
            return Some(maybe_key.to_string());
        }
        None
    }

    async fn get(
        &self,
        method: &str,
        params: &[(&str, String)]
    ) -> Option<Result<Response, Error>> {
        // Return None if there is no API key specified.
        if let Some(key) = self.get_api_key() {
            return Some(
                self.client
                    .get(API_ROOT)
                    .query(&[
                        ("format", "json"),
                        ("method", method),
                        ("api_key", key.as_ref())
                    ])
                    .query(params)
                    .send()
                    .await
            )
        }
        None
    }

    pub async fn get_album_info(&self, base_info: AlbumInfo) -> Result<String, &str> {
        // Either give a MusicBrainz album ID, or both album name & artist
        if let Some(id) = base_info.mb_album_id.as_ref() {
            if let Some(res) = self.get(
                "album.getinfo",
                &[
                    ("mbid", id)
                ]
            ).await {
                // TODO: Get summary
            }
        }
        if let Some(res) = self.get(
            "album.getinfo",
            &[
                ("artist", "")
            ]
        )
    }
}
