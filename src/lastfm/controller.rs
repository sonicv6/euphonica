extern crate bson;
extern crate html_escape;
use reqwest::{
    Client,
    Response,
    header::USER_AGENT
};
use gtk::prelude::*;

use crate::utils::settings_manager;

use super::models;

pub const API_ROOT: &str = "http://ws.audioscrobbler.com/2.0";

pub struct LastfmWrapper {
    client: Client
}

fn escape_nonlink_html(raw: &str) -> String {
    // Last.fm text content are not escaped (i.e. ampersands are kept verbatim)
    // YET they also contain <a> tags.
    // For proper Pango markup display, escape everything up to before the "Read
    // more..." link.
    match raw.find("<a href") {
        Some(index) => {
            let mut res = String::new();
            html_escape::encode_safe_to_string(&raw[..index], &mut res);
            res.push_str("\n");
            res.push_str(&raw[index..]);
            res
        },
        None => raw.to_owned()
    }
}

impl LastfmWrapper {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new()
        }
    }

    fn get_settings(&self) -> (String, String) {
        let settings = settings_manager().child("client");
        (
            settings.string("lastfm-api-key").to_string(),
            settings.string("lastfm-user-agent").to_string()
        )
    }

    async fn get(
        &self,
        method: &str,
        params: &[(&str, String)]
    ) -> Option<Response> {
        println!("Last.fm: calling `{}` with query {:?}", method, params);
        let (key, agent) = self.get_settings();
        // Return None if there is no API key specified.
        if !key.is_empty() && !agent.is_empty() {
            if let Ok(resp) = self
                .client
                .get(API_ROOT)
                .query(&[
                    ("format", "json"),
                    ("method", method),
                    ("api_key", key.as_ref())
                ])
                .query(params)
                .header(USER_AGENT, agent)
                .send()
                .await {
                    return Some(resp);
                }
            return None;
        }
        None
    }

    pub async fn get_album_meta(
        &self,
        key: bson::Document
    ) -> Result<models::Album, String> {
        // Will panic if key document is not a simple map of String to String
        let params: Vec<(&str, String)> = key.iter().map(
            |kv: (&String, &bson::Bson)| {
                (kv.0.as_ref(), kv.1.as_str().unwrap().to_owned())
            }
        ).collect();
        if let Some(resp) = self.get(
            "album.getinfo",
            &params
        ).await {
            // TODO: Get summary
            match resp.status() {
                reqwest::StatusCode::OK => {
                    match resp.json::<models::AlbumResponse>().await {
                        Ok(parsed) => {
                            // Some preprocessing is needed.
                            // Might have to put the mbid back in, as Last.fm won't return
                            // it if we queried using it in the first place.
                            let mut album: models::Album = parsed.album;
                            if let Some(id) = key.get("mbid") {
                                album.mbid = Some(id.as_str().unwrap().to_owned());
                            }
                            // Override album & artist names in case the returned values
                            // are slightly different (casing, apostrophes, etc.), else
                            // we won't be able to query it back using our own tags.
                            if let Some(artist) = key.get("artist") {
                                album.artist = artist.as_str().unwrap().to_owned();
                            }
                            if let Some(name) = key.get("album") {
                                album.name = name.as_str().unwrap().to_owned();
                            }
                            // Escape special characters in wiki entries except for
                            // the "read more" link tag.
                            if album.wiki.is_some() {
                                let raw_wiki = album.wiki.unwrap();
                                let new_wiki = models::Wiki {
                                    summary: escape_nonlink_html(&raw_wiki.summary),
                                    content:escape_nonlink_html(&raw_wiki.content),
                                };
                                album.wiki = Some(new_wiki);
                            }
                            Ok(album)

                        },
                        Err(err) => Err(format!("Failed to parse album: {:?}", err))
                    }
                }
                other => {
                    return Err(format!("[Last.fm] get_album_meta: status {:?}", other));
                }
            }
        }
        else {
            return Err("[Last.fm] get_album_meta: no response".to_owned());
        }
    }
}
