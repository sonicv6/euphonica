extern crate mpd;
use core::time::Duration;
use time::Date;
use std::path::Path;

// We define our own Song struct for more convenient handling, especially with
// regards to optional fields and tags such as albums.

#[derive(Debug, PartialEq)]
pub struct Song {
    uri: String,
    title: Option<String>,
    last_mod: Option<String>, // TODO: parse into time::Date
    artist: Option<String>,
    duration: Duration, // Default to 0 if somehow the option in mpd's Song is None
    place: Option<mpd::song::QueuePlace>,
    // range: Option<Range>,
    album: Option<String>,
    // TODO: add albumartist & albumsort
    release_date: Option<String>, // TODO: parse into time::Date
    // TODO: Add more fields for managing classical music, such as composer, ensemble and movement number
}

impl Song {
    // TODO: Might want a new() constructor too
    pub fn from_mpd_song(song: &mpd::song::Song) -> Self {
        // We don't want to clone the whole mpd Song object since there might
        // be fields that we won't ever use.
        let mut res = Self {
            uri: song.file.clone(),
            title: song.title.clone(),
            last_mod: song.last_mod.clone(),
            artist: song.artist.clone(),
            duration: song.duration.clone().unwrap_or(Duration::from_secs(0)),
            place: song.place.clone(),
            album: None,
            release_date: None
        };
        // Search tags vector for additional fields we can use.
        // Again we're using iter() here to avoid cloning everything.
        for (tag, val) in song.tags.iter() {
            match tag.as_str() {
                "album" => res.album = Some(val.clone()),
                "date" => res.release_date = Some(val.clone()),
                _ => {}
            }
        }
        res
    }

    pub fn get_title(&self) -> &Option<String> {
        &self.title
    }

    pub fn get_uri(&self) -> &String {
        &self.uri
    }

    pub fn get_name(&self) -> String {
        // Get title tag or filename without extension in case there's no title tag.
        // Returns a clone since
        // 1. Song names are (usually) short
        // 2. There might be no name tag, in which case we'll have to extract from the path.
        // Prefer song name in tag over filename
        if let Some(title) = self.title.as_ref() {
            return title.clone();
        }
        // Else extract from URI
        else if let Some(stem) = Path::new(&self.uri).file_stem() {
            return String::from(stem.to_str().unwrap());
        }
        String::from("Untitled")
    }

    pub fn get_duration(&self) -> Duration {
        self.duration.clone()
    }

    pub fn get_artist(&self) -> &Option<String> {
        &self.artist
    }
}