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
                    println!("{:?}", resp);
                    return Some(resp);
                }
            return None;
        }
        None
    }

    pub async fn get_album_info(
        &self,
        // Specify either this (preferred)
        mbid: Option<&str>,
        // Or BOTH of these
        album: Option<&str>, artist: Option<&str>
    ) -> Result<models::Album, String> {
        let params: Vec<(&str, String)>;
        if let Some(id) = mbid {
            params = vec!(("mbid", id.to_string()));
        }
        else if album.is_some() && artist.is_some() {
            params = vec!(
                ("album", album.unwrap().to_string()),
                ("artist", artist.unwrap().to_string())
            )
        }
        else {
            panic!("Lastfm: either mbid or both of album and artist must be specified");
        }
        if let Some(resp) = self.get(
            "album.getinfo",
            &params
        ).await {
            // TODO: Get summary
            match resp.status() {
                reqwest::StatusCode::OK => {
                    match resp.json::<models::AlbumResponse>().await {
                        Ok(parsed) => {
                            // Might have to put the mbid back in, as Last.fm won't return
                            // it if we queried using it in the first place.
                            let mut album: models::Album = parsed.album;
                            if let Some(id) = mbid {
                                album.mbid = Some(id.to_owned());
                            }
                            Ok(album)

                        },
                        Err(err) => Err(format!("Failed to parse album: {:?}", err))
                    }
                }
                other => {
                    return Err(format!("[Last.fm] get_album_info: status {:?}", other));
                }
            }
        }
        else {
            return Err("[Last.fm] get_album_info: no response".to_owned());
        }
    }
}
