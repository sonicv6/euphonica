extern crate bson;
use std::{
    sync::{Arc, Mutex},
    time::Duration
};
use gtk::{gio, glib};
use glib::clone;
use gio::spawn_blocking;

use async_channel::{Sender, Receiver};
use reqwest::{
    blocking::{Client, Response},
    header::USER_AGENT
};
use gtk::prelude::*;

use crate::utils::{
    settings_manager,
    resize_image
};

use super::super::{
    MetadataProvider, MetadataResponse, models, utils,
    prelude::*
};
use super::models::{LastfmAlbumResponse, LastfmArtistResponse};

pub const API_ROOT: &str = "http://ws.audioscrobbler.com/2.0";

enum Task {
    AlbumMeta(String, bson::Document), // folder_uri and key doc
    ArtistMeta(bson::Document)
}

mod background {
    use super::*;

    fn get_lastfm(
        client: Arc<Mutex<Client>>,
        method: &str,
        params: &[(&str, String)]
    ) -> Option<Response> {
        let settings = settings_manager().child("client");
        let key = settings.string("lastfm-api-key").to_string();
        let agent = settings.string("lastfm-user-agent").to_string();
        // Return None if there is no API key specified.
        if !key.is_empty() && !agent.is_empty() {
            println!("Last.fm: calling `{}` with query {:?}", method, params);
            let resp = client.lock().ok()?
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

    pub fn spawn_get_album_meta(
        client: Arc<Mutex<Client>>,
        key: bson::Document
    ) -> Result<models::AlbumMeta, String> {
        // Will panic if key document is not a simple map of String to String
        let params: Vec<(&str, String)> = key.iter().map(
            |kv: (&String, &bson::Bson)| {
                (kv.0.as_ref(), kv.1.as_str().unwrap().to_owned())
            }
        ).collect();

        if let Some(resp) = get_lastfm(
            client,
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
                            let mut album: models::AlbumMeta = parsed.album.into();
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

    pub fn spawn_get_artist_meta(
        client: Arc<Mutex<Client>>,
        key: bson::Document
    ) -> Result<models::ArtistMeta, String> {
        // Will panic if key document is not a simple map of String to String
        let params: Vec<(&str, String)> = key.iter().map(
            |kv: (&String, &bson::Bson)| {
                (if kv.0 == "name" { "artist" } else { kv.0.as_ref() }, kv.1.as_str().unwrap().to_owned())
            }
        ).collect();
        println!("Child thread: trying to fetch artist meta...");
        if let Some(resp) = get_lastfm(
            client,
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
                            let mut artist: models::ArtistMeta = parsed.artist.into();
                            if let Some(id) = key.get("mbid") {
                                artist.mbid = Some(id.as_str().unwrap().to_owned());
                            }
                            // Override artist name in case the returned values
                            // are slightly different (casing, apostrophes, etc.), else
                            // we won't be able to query it back using our own tags.
                            if let Some(name) = key.get("artist") {
                                artist.name = name.as_str().unwrap().to_owned();
                            }
                            Ok(artist)

                        },
                        Err(err) => Err(format!("Failed to parse artist: {:?}", err))
                    }
                }
                other => {
                    return Err(format!("[Last.fm] get_artist_meta: status {:?}", other));
                }
            }
        }
        else {
            return Err("[Last.fm] get_album_meta: no response".to_owned());
        }
    }
}

pub struct LastfmWrapper {
    task_sender: Sender<Task>
}

impl MetadataProvider for LastfmWrapper {
    fn new(result_sender: Sender<MetadataResponse>) -> Self {
        let (task_sender, receiver): (
            Sender<Task>,
            Receiver<Task>
        ) = async_channel::unbounded();
        let res = Self {
            task_sender
        };
        res.setup_channel(receiver, result_sender);
        res
    }

    /// Schedule getting album metadata from Last.fm.
    /// A signal will be emitted to notify the caller when the result arrives.
    fn get_album_meta(
        &self,
        folder_uri: &str,
        key: bson::Document
    ) {
        let _ = self.task_sender.send_blocking(Task::AlbumMeta(folder_uri.to_string(), key));
    }

    /// Schedule getting artist metadata from Last.fm.
    /// A signal will be emitted to notify the caller when the result arrives.
    fn get_artist_meta(
        &self,
        key: bson::Document
    ) {
        let _ = self.task_sender.send_blocking(Task::ArtistMeta(key));
    }
}

impl LastfmWrapper {
    fn setup_channel(&self, receiver: Receiver<Task>, result_sender: Sender<MetadataResponse>) {
        // Schedule tasks, enforcing a safe delay between requests.
        // Set up a listener for updates from metadata providers.
        glib::MainContext::default().spawn_local(
            async move {
                use futures::prelude::*;
                // Allow receiver to be mutated, but keep it at the same memory address.
                // See Receiver::next doc for why this is needed.
                let mut receiver = std::pin::pin!(receiver);
                let client = Arc::new(Mutex::new(Client::new()));
                let settings = settings_manager().child("client");
                while let Some(request) = receiver.next().await {
                    match request {
                        Task::AlbumMeta(folder_uri, key) => {
                            let _ = spawn_blocking(clone!(
                                #[weak]
                                client,
                                #[strong]
                                result_sender,
                                move || {
                                    let res = background::spawn_get_album_meta(client.clone(), key);
                                    if let Ok(album) = res {
                                        // Do this first while we still have the album meta.
                                        // if settings_manager()
                                        //     .child("client")
                                        //     .boolean("lastfm-download-album-art") {
                                        //     let urls = album.get_images();
                                        //     let res = utils::get_best_image(client, &urls);
                                        //     if let Ok(dyn_img) = res {
                                        //         let _ = result_sender.send_blocking(
                                        //             MetadataResponse::AlbumArt(folder_uri.clone(), dyn_img)
                                        //         );
                                        //     }
                                        //     else {
                                        //         println!("Last.fm: could not get album art. {}", res.err().unwrap());
                                        //     }
                                        // }
                                        // Album meta moved
                                        let _ = result_sender.send_blocking(MetadataResponse::AlbumMeta(folder_uri, album));
                                    }
                                    else {
                                        println!("{}", res.err().unwrap());
                                    }
                                }
                            )).await;
                        },
                        Task::ArtistMeta(key) => {
                            // Must provide the artist name field
                            if let Some(name_bson) = key.get("name") {
                                let name = name_bson.as_str().unwrap().to_owned();
                                let _ = spawn_blocking(clone!(
                                    #[weak]
                                    client,
                                    #[strong]
                                    result_sender,
                                    move || {
                                        let res = background::spawn_get_artist_meta(client.clone(), key);
                                        if let Ok(meta) = res {
                                            // Do this while we still have the artist meta struct
                                            if settings_manager()
                                                .child("client")
                                                .boolean("lastfm-download-artist-avatar") {
                                                    let urls = meta.get_images().to_vec();
                                                    let res = utils::get_best_image(client, &urls);
                                                    if let Ok(dyn_img) = res {
                                                        let (hires, thumb) = resize_image(dyn_img);
                                                        let _ = result_sender.send_blocking(
                                                            MetadataResponse::ArtistAvatar(
                                                                name.clone(),
                                                                hires,
                                                                thumb
                                                            )
                                                        );
                                                    }
                                                    else {
                                                        println!("Last.fm: could not get artist avatar. {}", res.err().unwrap());
                                                    }
                                                }
                                            // Artist meta moved
                                            let _ = result_sender.send_blocking(MetadataResponse::ArtistMeta(name, meta));
                                        }
                                        else {
                                            println!("{}", res.err().unwrap());
                                        }
                                    }
                                )).await;
                            }
                        }
                    };
                    glib::timeout_future(Duration::from_millis(
                        (settings.double("lastfm-delay-between-requests-s") * 1000.0) as u64
                    )).await;
                }
            }
        );
    }
}
