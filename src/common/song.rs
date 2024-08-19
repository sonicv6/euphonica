use time::{Date, Month};
use core::time::Duration;
use std::{
    path::Path,
    cell::{Ref, Cell, RefCell},
    ops::Deref,
    ffi::OsStr
};
use gtk::glib;
use gtk::gdk::Texture;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use mpd::status::AudioFormat;

use crate::utils::strip_filename_linux;

use super::{
    ArtistInfo,
    AlbumInfo,
    parse_mb_artist_tag,
    artists_to_string
};

// Mostly for eyecandy
#[derive(Clone, Copy, Debug, glib::Enum, PartialEq, Default)]
#[enum_type(name = "EuphoniaQualityGrade")]
pub enum QualityGrade {
    #[default]
    Unknown, // Catch-all
    Lossy,  // Anything not meeting the below
    CD,  // Lossless codec (FLAC, WavPack & Monkey's Audio for now) 44100-48000Hz 16bit.
    // While 48000Hz isn't technically Red Book, the "quality" should
    // be the same (unless resampled from 44100Hz CD).
    HiRes, // Lossless codec above 48000Hz and at least 24 bit depth.
    DSD // 150MB song files go brrr
}

impl QualityGrade {
    pub fn to_icon_name(self) -> Option<&'static str> {
        match self {
            Self::Unknown => None,
            Self::Lossy => None,
            Self::CD => Some("format-cd-symbolic"),
            Self::HiRes => Some("format-hires-symbolic"),
            Self::DSD => Some("format-dsd-symbolic")
        }
    }
}


fn parse_date(datestr: &str) -> Option<Date> {
    // MPD uses yyyy-MM-dd but the month and day may be optional.
    let mut comps = datestr.split('-');
    let mut year_val: Option<i32> = None;
    let mut month_val: Month = Month::January;
    let mut day_val: u8 = 1;

    let year_str = comps.next()?;
    if let Ok(year) = year_str.parse::<i32>() {
        let _ = year_val.replace(year);
    }
    else {
        return None;
    }

    if let Some(month_str) = comps.next() {
        if let Ok(month) = month_str.parse::<u8>() {
            if let Ok(month_enum) = Month::try_from(month) {
                month_val = month_enum;
            }
        }
    }
    if let Some(day_str) = comps.next() {
        if let Ok(day) = day_str.parse::<u8>() {
            day_val = day;
        }
    }
    if let Ok(date) = Date::from_calendar_date(
        year_val.unwrap(),
        month_val,
        day_val
    ) {
        return Some(date);
    }
    None
}

/// We define our own Song struct for more convenient handling, especially with
/// regards to optional fields and tags such as albums.
#[derive(Debug, Clone)]
pub struct SongInfo {
    // TODO: Might want to refactor to Into<Cow<'a, str>>
    uri: String,
    title: Option<String>,
    // last_mod: RefCell<Option<u64>>,
    artists: Vec<ArtistInfo>,
    album_artists: Vec<ArtistInfo>,
    duration: Option<Duration>, // Default to 0 if somehow the option in mpd's Song is None
    queue_id: Option<u32>,
    // range: Option<Range>,
    album: Option<AlbumInfo>,
    track: Cell<i64>,
    disc: Cell<i64>,
    // TODO: add albumsort
    // Store Date instead of string to save a tiny bit of memory.
    // Also gives us formatting flexibility in the future.
    release_date: Option<Date>,
    // TODO: Add more fields for managing classical music, such as composer, ensemble and movement number
    is_playing: Cell<bool>,
    thumbnail: Option<Texture>,
    quality_grade: QualityGrade,
    // MusicBrainz stuff
    mbid: Option<String>,
}

impl SongInfo {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for SongInfo {
    fn default() -> Self {
        Self {
            uri: String::from(""),
            title: None,
            artists: Vec::new(),
            album_artists: Vec::new(),
            duration: None,
            queue_id: None,
            album: None,
            track: Cell::new(-1),
            disc: Cell::new(-1),
            release_date: None,
            is_playing: Cell::new(false),
            thumbnail: None,
            quality_grade: QualityGrade::Unknown,
            mbid: None,
        }
    }
}


mod imp {
    use glib::{
        ParamSpec,
        ParamSpecUInt,
        ParamSpecUInt64,
        ParamSpecInt64,
        ParamSpecBoolean,
        ParamSpecString,
        ParamSpecObject
    };
    use once_cell::sync::Lazy;
    use super::*;

    #[derive(Default, Debug)]
    pub struct Song {
        pub info: RefCell<SongInfo>
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Song {
        const NAME: &'static str = "EuphoniaSong";
        type Type = super::Song;

        fn new() -> Self {
            Self {
                info: RefCell::new(SongInfo::default())
            }
        }
    }

    impl ObjectImpl for Song {
        fn properties() -> &'static [ParamSpec] {
            static PROPERTIES: Lazy<Vec<ParamSpec>> = Lazy::new(|| {
                vec![
                    ParamSpecString::builder("uri").read_only().build(),
                    ParamSpecString::builder("name").read_only().build(),
                    // ParamSpecString::builder("last_mod").build(),
                    ParamSpecString::builder("artist").read_only().build(),
                    ParamSpecString::builder("album-artist").read_only().build(),
                    ParamSpecUInt64::builder("duration").read_only().build(),
                    ParamSpecUInt::builder("queue-id").build(),
                    ParamSpecBoolean::builder("is-queued").read_only().build(),
                    ParamSpecString::builder("album").read_only().build(),
                    ParamSpecInt64::builder("track").read_only().build(),
                    ParamSpecInt64::builder("disc").read_only().build(),
                    ParamSpecObject::builder::<glib::BoxedAnyObject>("release-date").read_only().build(),  // boxes Option<time::Date>
                    ParamSpecString::builder("quality-grade").read_only().build(),
                    ParamSpecObject::builder::<Texture>("thumbnail")
                        .read_only()
                        .build(),
                ]
            });
            PROPERTIES.as_ref()
        }

        fn property(&self, _id: usize, pspec: &ParamSpec) -> glib::Value {
            let obj = self.obj();
            match pspec.name() {
                "uri" => obj.get_uri().to_value(),
                "name" => obj.get_name().to_value(),
                // "last_mod" => obj.get_last_mod().to_value(),
                // Represented in MusicBrainz format, i.e. Composer; Performer, Performer,...
                // The composer part is optional.
                "artist" => obj.get_artist_str().to_value(),
                "album-artist" => obj.get_album_artist_str().to_value(),
                "duration" => obj.get_duration().to_value(),
                "queue-id" => obj.get_queue_id().to_value(),
                "is-queued" => obj.is_queued().to_value(),
                "album" => obj.get_album_title().to_value(),
                "track" => obj.get_track().to_value(),
                "disc" => obj.get_disc().to_value(),
                "release-date" => glib::BoxedAnyObject::new(obj.get_release_date()).to_value(),
                // "release_date" => obj.get_release_date.to_value(),
                "is-playing" => obj.is_playing().to_value(),
                "quality-grade" => obj.get_quality_grade().to_value(),
                "thumbnail" => obj.get_thumbnail().to_value(),
                _ => unimplemented!(),
            }
        }

        // fn set_property(&self, _id: usize, value: &glib::Value, pspec: &ParamSpec) {
        //     let obj = self.obj();
        //     match pspec.name() {
        //         _ => unimplemented!(),
        //     }
        // }
    }
}

glib::wrapper! {
    pub struct Song(ObjectSubclass<imp::Song>);
}

impl Song {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn get_uri(&self) -> String {
        self.imp().info.borrow().uri.clone()
    }

    pub fn get_name(&self) -> Option<String> {
        // Get title tag or filename without extension in case there's no title tag.
        // Prefer song name in tag over filename
        if let Some(title) = self.imp().info.borrow().title.as_ref() {
            return Some(title.clone());
        }
        // Else extract from URI
        else if let Some(stem) = Path::new(&self.get_uri()).file_stem() {
            return Some(String::from(stem.to_str().unwrap()));
        }
        None
    }

    pub fn get_duration(&self) -> u64 {
        if let Some(dur) = self.imp().info.borrow().duration.as_ref() {
            return dur.as_secs();
        }
        0
    }

    pub fn get_artists(&self) -> Vec<ArtistInfo> {
        self.imp().info.borrow().artists.clone()
    }

    pub fn get_artist_str(&self) -> Option<String> {
        artists_to_string(&self.imp().info.borrow().artists)
    }

    pub fn get_album_artists(&self) -> Vec<ArtistInfo> {
        self.imp().info.borrow().album_artists.clone()
    }

    pub fn get_album_artist_str(&self) -> Option<String> {
        artists_to_string(&self.imp().info.borrow().album_artists)
    }

    pub fn get_queue_id(&self) -> u32 {
        if let Some(id) = self.imp().info.borrow().queue_id {
            return id;
        }
        0
    }

    pub fn is_queued(&self) -> bool {
        self.imp().info.borrow().queue_id.is_some()
    }

    pub fn get_album(&self) -> Option<AlbumInfo> {
        self.imp().info.borrow().album.clone()
    }

    pub fn get_album_title(&self) -> Option<String> {
        if let Some(album) = &self.imp().info.borrow().album {
            Some(album.title.clone())
        }
        else {
            None
        }
    }

    pub fn get_track(&self) -> i64 {
        self.imp().info.borrow().track.get()
    }

    pub fn get_disc(&self) -> i64 {
        self.imp().info.borrow().disc.get()
    }

    pub fn get_thumbnail(&self) -> Option<Texture> {
        self.imp().info.borrow().thumbnail.clone()
    }

    pub fn set_thumbnail(&self, tex: Option<Texture>) {
        {
            let mut info = self.imp().info.borrow_mut();
            info.thumbnail = tex;
        }
        self.notify("thumbnail");
    }

    pub fn is_playing(&self) -> bool {
        self.imp().info.borrow().is_playing.get()
    }

    pub fn set_is_playing(&self, val: bool) {
        let old_val = self.imp().info.borrow().is_playing.replace(val);
        if old_val != val {
            self.notify("is-playing");
        }
    }

    pub fn get_quality_grade(&self) -> QualityGrade {
        self.imp().info.borrow().quality_grade
    }

    pub fn get_release_date(&self) -> Option<Date> {
        self.imp().info.borrow().release_date
    }

    pub fn get_mbid(&self) -> Option<String> {
        self.imp().info.borrow().mbid.clone()
    }
}

impl Default for Song {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl From<mpd::song::Song> for SongInfo {
    fn from(song: mpd::song::Song) -> Self {
        let artists: Vec<ArtistInfo>;
        if let Some(artist_str) = song.artist {
            artists = parse_mb_artist_tag(&artist_str);
        }
        else {
            artists = Vec::with_capacity(0);
        }
        let mut res = Self {
            uri: song.file,
            title: song.title,
            artists,
            album_artists: Vec::with_capacity(0),
            duration: song.duration,
            queue_id: None,
            album: None,
            track: Cell::new(-1),
            disc: Cell::new(-1),
            release_date: None,
            is_playing: Cell::new(false),
            thumbnail: None,
            quality_grade: QualityGrade::Unknown,
            mbid: None
        };
        if let Some(place) = song.place {
            let _ = res.queue_id.replace(place.id.0);
        }

        // Search tags vector for additional fields we can use.
        // Again we're using iter() here to avoid cloning everything.
        // Limitation: MPD cannot parse DSD song format UNTIL PLAYED.
        // As such, DSD format description must be handled by the player controller,
        // working off the "format" attribute of the Status object.
        // The bits == 1 check only works with htkhiem's fork of rust-mpd with DSD correction
        let maybe_extension = Path::new(&res.uri).extension().and_then(OsStr::to_str);
        if let Some(extension) = maybe_extension {
            if ["dsf", "dff", "wsd"].contains(&extension) {
                // Is probably DSD
                res.quality_grade = QualityGrade::DSD;
            }
        }
        let mut artist_mbids: Vec<String> = Vec::new();
        let mut album_artist_str: Option<String> = None;
        let mut album_artist_mbids: Vec<String> = Vec::new();
        let mut album_mbid: Option<String> = None;
        for (tag, val) in song.tags.into_iter() {
            match tag.to_lowercase().as_str() {
                "album" => {
                    if res.album.is_none() {
                        let _ = res.album.replace(
                            AlbumInfo::new(
                                strip_filename_linux(&res.uri),
                                &val,
                                Vec::with_capacity(0)
                            )
                        );
                    }
                    else {
                        panic!("Multiple Album tags found. Only one per song is supported.");
                    }
                },
                "albumartist" => {
                    if album_artist_str.is_none() {
                        let _ = album_artist_str.replace(val);
                    }
                    else {
                        panic!("Multiple AlbumArtist tags found. Only one per song is supported (use MusicBrainz syntax to specify multiple artists).");
                    }
                },
                // "date" => res.imp().release_date.replace(Some(val.clone())),
                "format" => {
                    if let Some(extension) = maybe_extension {
                        if let Ok(format) = val.parse::<AudioFormat>() {
                            if ["flac", "alac", "wv", "ape"].contains(&extension) {
                                // Is probably lossless PCM
                                if format.rate > 48000 && format.bits >= 24 {
                                    res.quality_grade = QualityGrade::HiRes;
                                }
                                else {
                                    res.quality_grade = QualityGrade::CD;
                                }
                            }
                            else {
                                res.quality_grade = QualityGrade::Lossy;
                            }
                        }
                    }
                },
                "originaldate" => {
                    res.release_date = parse_date(val.as_ref());
                },
                "track" => {
                    if let Ok(idx) = val.parse::<i64>() {
                        let _ = res.track.replace(idx);
                    }
                }
                "disc" => {
                    if let Ok(idx) = val.parse::<i64>() {
                        let _ = res.disc.replace(idx);
                    }
                }
                // Beets might use uppercase versions of these keys but we're
                // converting all to lowercase
                "musicbrainz_trackid" => {
                    let _ = res.mbid.replace(val);
                }
                "musicbrainz_albumid" => {
                    // Can encounter this before initialising the album object
                    if album_mbid.is_none() {
                        let _ = album_mbid.replace(val);
                    }
                    else {
                        panic!("Multiple musicbrainz_albumid tags found. Only one per song is supported.");
                    }
                }
                "musicbrainz_artistid" => {
                    // Can encounter this multiple times and/or before
                    // initialising the artist objects
                    artist_mbids.push(val);
                }
                "musicbrainz_albumartistid" => {
                    // Can encounter this multiple times and/or before
                    // initialising the albumartist objects
                    album_artist_mbids.push(val);
                }
                _ => {}
            }
        }

        // Assume the artist IDs are given in the same order as the artist tags
        for (idx, id) in artist_mbids.drain(..).enumerate() {
            if idx < res.artists.len() {
                let _ = res.artists[idx].mbid.replace(id);
            }
        }

        if let Some(album) = res.album.as_mut() {
            album.mbid = album_mbid;
            // Assume the albumartist IDs are given in the same order as the albumartist tags
            if let Some(s) = album_artist_str.as_mut() {
                album.set_artists_from_string(s);
                for (idx, id) in album_artist_mbids.drain(..).enumerate() {
                    if idx < album.artists.len() {
                        let _ = album.artists[idx].mbid.replace(id);
                    }
                }
            }
        }

        res
    }
}

impl From<mpd::song::Song> for Song {
    fn from(song: mpd::song::Song) -> Self {
        let info = SongInfo::from(song);
        let res = glib::Object::new::<Self>();
        res.imp().info.replace(info);
        res
    }
}
