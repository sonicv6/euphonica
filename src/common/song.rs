extern crate mpd;
use core::time::Duration;
use time::Date;
use std::{
    path::{Path, PathBuf},
    cell::{Cell, RefCell}
};
use chrono::NaiveDate;
use glib::Properties;
use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

// fn parse_date(datestr: &str) -> Option<NaiveDate> {
//     let mut comps = datestr.split("-");
//     if let Some(year_str) = comps.next() {
//         if let Ok(year) = year_str.parse::<i32>() {
//             if let Some(month_str) = comps.next() {
//                 if let Ok(month) = month_str.parse::<u32>() {
//                     if let Some(day_str) = comps.next() {
//                         if let Ok(day) = day_str.parse::<u32>() {
//                             return NaiveDate::from_ymd_opt(year, month, day);
//                         }
//                         return NaiveDate::from_ymd_opt(year, month, 1);
//                     }
//                     return NaiveDate::from_ymd_opt(year, month, 1);
//                 }
//                 return NaiveDate::from_ymd_opt(year, 1, 1);
//             }
//             return NaiveDate::from_ymd_opt(year, 1, 1);
//         }
//         return None;
//     }
//     None
// }

// We define our own Song struct for more convenient handling, especially with
// regards to optional fields and tags such as albums.

mod imp {
    use glib::{
        ParamSpec,
        ParamSpecUInt,
        ParamSpecUInt64,
        ParamSpecBoolean,
        ParamSpecString,
    };
    use once_cell::sync::Lazy;
    use super::*;

    #[derive(Default, Debug)]
    pub struct Song {
        // These are all cells to allow for modification by the ID3 tag editor
        pub uri: RefCell<String>,
        pub title: RefCell<Option<String>>,
        // pub last_mod: RefCell<Option<u64>>,
        pub artist: RefCell<Option<String>>,
        pub duration: Cell<u64>, // Default to 0 if somehow the option in mpd's Song is None
        pub queue_id: Cell<Option<u32>>,
        // range: Option<Range>,
        pub album: RefCell<Option<String>>,
        // TODO: add albumartist & albumsort
        // pub release_date: RefCell<Option<u64>>,
        pub thumbnail_path: RefCell<Option<String>>,
        pub cover_path: RefCell<Option<String>>,
        // TODO: Add more fields for managing classical music, such as composer, ensemble and movement number
        pub is_playing: Cell<bool>
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Song {
        const NAME: &'static str = "SlamprustSong";
        type Type = super::Song;

        fn new() -> Self {
            Self {
                uri: RefCell::new(String::from("")),
                title: RefCell::new(None),
                artist: RefCell::new(None),
                duration: Cell::new(0),
                queue_id: Cell::new(None),
                album: RefCell::new(None),
                cover_path: RefCell::new(None),
                thumbnail_path: RefCell::new(None),
                is_playing: Cell::new(false)
            }
        }
    }

    impl ObjectImpl for Song {
        fn properties() -> &'static [ParamSpec] {
            static PROPERTIES: Lazy<Vec<ParamSpec>> = Lazy::new(|| {
                vec![
                    ParamSpecString::builder("uri").construct_only().build(),
                    ParamSpecString::builder("name").build(),
                    // ParamSpecString::builder("last_mod").build(),
                    ParamSpecString::builder("artist").build(),
                    ParamSpecUInt64::builder("duration").construct_only().build(),
                    ParamSpecUInt::builder("queue-id").build(),
                    ParamSpecBoolean::builder("is-queued").read_only().build(),
                    ParamSpecString::builder("album").build(),
                    // ParamSpecString::builder("release_date").build(),
                    ParamSpecString::builder("cover-path").build(),
                    ParamSpecString::builder("thumbnail-path").build(),
                    ParamSpecBoolean::builder("is-playing").build(),
                    // ParamSpecObject::builder::<gdk::Texture>("cover")
                    //     .read_only()
                    //     .build(),
                ]
            });
            PROPERTIES.as_ref()
        }

        fn property(&self, _id: usize, pspec: &ParamSpec) -> glib::Value {
            let obj = self.obj();
            match pspec.name() {
                "uri" => obj.get_uri().to_value(),
                // These are proxies for Song properties
                "name" => obj.get_name().to_value(),
                // "last_mod" => obj.get_last_mod().to_value(),
                "artist" => obj.get_artist().to_value(),
                // "album" => obj.album().to_value(),
                "duration" => obj.get_duration().to_value(),
                "queue-id" => obj.get_queue_id().to_value(),
                "is-queued" => obj.is_queued().to_value(),
                "album" => obj.get_album().to_value(),
                // "release_date" => obj.get_release_date.to_value(),
                "cover-path" => obj.get_cover_path(false).to_value(),
                "thumbnail-path" => obj.get_cover_path(true).to_value(),
                "is-playing" => obj.is_playing().to_value(),
                _ => unimplemented!(),
            }
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &ParamSpec) {
            let obj = self.obj();
            match pspec.name() {
                "uri" => {
                    if let Ok(uri) = value.get::<&str>() {
                        let _ = self.uri.replace(uri.to_owned());
                    }
                }
                "duration" => {
                    if let Ok(dur) = value.get::<u64>() {
                        let _ = self.duration.replace(dur);
                    }
                }
                "name" => {
                    // Always set to title tag
                    if let Ok(name) = value.get::<&str>() {
                        let _ = self.title.replace(Some(name.to_owned()));
                    }
                    obj.notify("name");
                }
                "artist" => {
                    if let Ok(a) = value.get::<&str>() {
                        let _ = self.artist.replace(Some(a.to_owned()));
                    }
                    obj.notify("artist");
                }
                // "queue-id" => {
                //     if let Ok(id) = value.get::<u32>() {
                //         let _ = self.queue_id.replace(Some(id));
                //     }
                //     obj.notify("queue-id");
                //     obj.notify("is-queued");
                // }
                "album" => {
                    if let Ok(album) = value.get::<&str>() {
                        let _ = self.album.replace(Some(album.to_owned()));
                    }
                    obj.notify("album");
                }
                "cover-path" => {
                    if let Ok(c) = value.get::<&str>() {
                        let _ = self.cover_path.replace(Some(c.to_owned()));
                    }
                    obj.notify("cover-path");
                },
                "thumbnail-path" => {
                    if let Ok(c) = value.get::<&str>() {
                        let _ = self.thumbnail_path.replace(Some(c.to_owned()));
                    }
                    obj.notify("thumbnail-path");
                },
                "is-playing" => {
                    if let Ok(b) = value.get::<bool>() {
                        let _ = self.is_playing.replace(b);
                        obj.notify("is-playing");
                    }
                }
                _ => unimplemented!(),
            }
        }
    }
}

glib::wrapper! {
    pub struct Song(ObjectSubclass<imp::Song>);
}

impl Song {
    // TODO: Might want a new() constructor too
    pub fn from_mpd_song(song: &mpd::song::Song) -> Self {
        // We don't want to clone the whole mpd Song object since there might
        // be fields that we won't ever use.
        let res = glib::Object::builder::<Self>()
            .property("uri", song.file.clone())
            .property("name", song.title.clone())
            //.property("last_mod", song.last_mod.clone())
            .property("artist", song.artist.clone())
            .property("duration", song.duration.expect("Song must have duration").as_secs())
            // .property("album", None)
            // .property("cover_hash", None)
            //.property("release_date", None)
            .build();

        if let Some(place) = song.place {
            let _ = res.imp().queue_id.replace(Some(place.id.0));
        }

        // Search tags vector for additional fields we can use.
        // Again we're using iter() here to avoid cloning everything.
        for (tag, val) in song.tags.iter() {
            match tag.as_str() {
                "Album" => {let _ = res.imp().album.replace(Some(val.clone()));},
                // "date" => res.imp().release_date.replace(Some(val.clone())),
                _ => {}
            }
        }
        res
    }

    pub fn get_uri<'a>(&self) -> String {
        self.imp().uri.borrow().clone()
    }

    pub fn get_name(&self) -> String {
        // Get title tag or filename without extension in case there's no title tag.
        // Returns a clone since
        // 1. Song names are (usually) short
        // 2. There might be no name tag, in which case we'll have to extract from the path.
        // Prefer song name in tag over filename
        if let Some(title) = self.imp().title.borrow().as_ref() {
            return title.clone();
        }
        // Else extract from URI
        else if let Some(stem) = Path::new(&self.get_uri()).file_stem() {
            return String::from(stem.to_str().unwrap());
        }
        String::from("Untitled")
    }

    pub fn get_duration(&self) -> u64 {
        self.imp().duration.get()
    }

    pub fn get_artist(&self) -> String {
        if let Some(artist) = self.imp().artist.borrow().clone() {
            return artist;
        }
        String::from("Unknown")
    }

    pub fn get_queue_id(&self) -> u32 {
        if let Some(id) = self.imp().queue_id.get() {
            return id;
        }
        0
    }

    pub fn is_queued(&self) -> bool {
        self.imp().queue_id.get().is_some()
    }

    pub fn get_album(&self) -> String {
        if let Some(album) = self.imp().album.borrow().clone() {
            return album;
        }
        String::from("Unknown")
    }

    pub fn get_cover_path(&self, thumbnail: bool) -> Option<String> {
        if thumbnail {
            return self.imp().thumbnail_path.borrow().clone();
        }
        self.imp().cover_path.borrow().clone()
    }

    pub fn set_cover_path(&self, path: &PathBuf, thumbnail: bool) {
        if thumbnail {
            let _ = self.imp()
                .thumbnail_path
                .replace(Some(
                    path.to_str()
                    .expect("Invalid thumbnail path!")
                    .to_owned()
            ));
            self.notify("thumbnail-path");
        }
        else {
            let _ = self.imp()
                .cover_path
                .replace(Some(
                    path.to_str()
                    .expect("Invalid cover path!")
                    .to_owned()
            ));
            self.notify("cover-path");
        }
    }

    pub fn has_cover(&self) -> bool {
        self.imp().thumbnail_path.borrow().is_some() && self.imp().cover_path.borrow().is_some()
    }

    pub fn is_playing(&self) -> bool {
        self.imp().is_playing.get()
    }

    pub fn set_is_playing(&self, val: bool) {
        let old_val = self.imp().is_playing.replace(val);
        if old_val != val {
            self.notify("is-playing");
        }
    }
}

impl Default for Song {
    fn default() -> Self {
        glib::Object::new()
    }
}