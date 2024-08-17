extern crate bson;
use std::{
    sync::{Arc, Mutex},
    rc::Rc,
    time::Duration
};
use gtk::{gio, glib};
use glib::clone;
use gio::spawn_blocking;
use html_escape;
use async_channel::{Sender, Receiver};
use reqwest::{
    blocking::{Client, Response},
    header::USER_AGENT
};
use gtk::prelude::*;

use crate::{
    utils::settings_manager,
    cache::AsyncResponse
};

use super::models;

pub const API_ROOT: &str = "http://ws.audioscrobbler.com/2.0";

pub struct LastfmWrapper {
    task_sender: Sender<Task>,
    result_sender: Sender<AsyncResponse>
}

enum Task {
    AlbumMeta(String, bson::Document), // folder_uri and key doc
    ArtistMeta(bson::Document)
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

fn spawn_get(
    client: Arc<Mutex<Client>>,
    method: &str,
    params: &[(&str, String)]
) -> Option<Response> {
    println!("Last.fm: calling `{}` with query {:?}", method, params);
    let settings = settings_manager().child("client");
    let key = settings.string("lastfm-api-key").to_string();
    let agent = settings.string("lastfm-user-agent").to_string();
    // Return None if there is no API key specified.
    if !key.is_empty() && !agent.is_empty() {
        if let Ok(resp) = client.lock().ok()?
            .get(API_ROOT)
            .query(&[
                ("format", "json"),
                ("method", method),
                ("api_key", key.as_ref())
            ])
            .query(params)
            .header(USER_AGENT, agent)
            .send() {
                return Some(resp);
            }
        return None;
    }
    None
}

fn spawn_get_album_meta(
    client: Arc<Mutex<Client>>,
    key: bson::Document
) -> Result<models::Album, String> {
    // Will panic if key document is not a simple map of String to String
    let params: Vec<(&str, String)> = key.iter().map(
        |kv: (&String, &bson::Bson)| {
            (kv.0.as_ref(), kv.1.as_str().unwrap().to_owned())
        }
    ).collect();

    if let Some(resp) = spawn_get(
        client,
        "album.getinfo",
        &params
    ) {
        // TODO: Get summary
        match resp.status() {
            reqwest::StatusCode::OK => {
                match resp.json::<models::AlbumResponse>() {
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

impl LastfmWrapper {
    pub fn new(result_sender: Sender<AsyncResponse>) -> Rc<Self> {
        let (task_sender, receiver): (
            Sender<Task>,
            Receiver<Task>
        ) = async_channel::unbounded();
        let res = Rc::new(Self {
            task_sender,
            result_sender: result_sender.clone()
        });
        res.clone().setup_channel(receiver, result_sender);
        res
    }

    fn setup_channel(self: Rc<Self>, receiver: Receiver<Task>, result_sender: Sender<AsyncResponse>) {
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
                        Task::AlbumMeta(folder_uri, key) => spawn_blocking(clone!(
                            #[weak]
                            client,
                            #[strong]
                            result_sender,
                            move || {
                                if let Ok(album) = spawn_get_album_meta(client, key) {
                                    let _ = result_sender.send_blocking(AsyncResponse::Album(folder_uri, album));
                                }
                            }
                        )).await,
                        Task::ArtistMeta(_) => unimplemented!()
                    };
                    println!("le sleep");
                    glib::timeout_future(Duration::from_millis(
                        (settings.double("lastfm-delay-between-requests-s") * 1000.0) as u64
                    )).await;
                    println!("le wake");
                }
            }
        );
    }

    /// Schedule getting album metadata from Last.fm.
    /// A signal will be emitted to notify the caller when the result arrives.
    pub fn get_album_meta(
        &self,
        folder_uri: &str,
        key: bson::Document
    ) {
        let _ = self.task_sender.send_blocking(Task::AlbumMeta(folder_uri.to_string(), key));
    }
}
