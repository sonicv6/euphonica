use std::cell::OnceCell;
use time::Date;
use gtk::glib;
use gtk::gdk::Texture;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

use super::{
    QualityGrade,
    ArtistInfo,
    SongInfo,
    parse_mb_artist_tag,
    artists_to_string
};

// This is a model class for queue view displays.
// It does not contain any actual song in terms of data.

#[derive(Debug, Clone, PartialEq)]
pub struct AlbumInfo {
    // TODO: Might want to refactor to Into<Cow<'a, str>>
    pub title: String,
    // Folder-based URI, acquired from the first song found with this album's tag.
    pub uri: String,
    pub artists: Vec<ArtistInfo>, // parse from AlbumArtist tag please, not Artist.
    pub cover: Option<Texture>,
    pub release_date: Option<Date>,
    pub quality_grade: QualityGrade,
    pub mbid: Option<String>
}

impl AlbumInfo {
    pub fn new(uri: &str, title: &str, artists: Vec<ArtistInfo>) -> Self {
        Self {
            uri: uri.to_owned(),
            artists,
            title: title.to_owned(),
            cover: None,
            release_date: None,
            quality_grade: QualityGrade::Unknown,
            mbid: None
        }
    }

    pub fn set_artists_from_string(&mut self, tag: &str) {
        self.artists = parse_mb_artist_tag(tag);
    }
}

impl Default for AlbumInfo {
    fn default() -> Self {
        AlbumInfo {
            title: "Untitled Album".to_owned(),
            uri: "".to_owned(),
            artists: Vec::with_capacity(0),
            cover: None,
            release_date: None,
            quality_grade: QualityGrade::Unknown,
            mbid: None
        }
    }
}

impl From<SongInfo> for AlbumInfo {
    fn from(song_info: SongInfo) -> Self {
        song_info.into_album_info().unwrap()
    }
}

mod imp {
    use glib::{
        ParamSpec,
        // ParamSpecUInt,
        // ParamSpecUInt64,
        // ParamSpecBoolean,
        ParamSpecString,
        ParamSpecObject
    };
    use once_cell::sync::Lazy;
    use super::*;

    /// The GObject Song wrapper.
    /// By nesting info inside another struct, we enforce tag editing to be
    /// atomic. Tag editing is performed by first cloning the whole SongInfo
    /// struct to a mutable variable, modify it, then create a new Song wrapper
    /// from the modified SongInfo struct (no copy required this time).
    /// This design also avoids a RefCell.
    #[derive(Default, Debug)]
    pub struct Album {
        pub info: OnceCell<AlbumInfo>
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Album {
        const NAME: &'static str = "EuphoniaAlbum";
        type Type = super::Album;

        fn new() -> Self {
            Self {
                info: OnceCell::new()
            }
        }
    }

    impl ObjectImpl for Album {
        fn properties() -> &'static [ParamSpec] {
            static PROPERTIES: Lazy<Vec<ParamSpec>> = Lazy::new(|| {
                vec![
                    ParamSpecString::builder("uri").read_only().build(),
                    ParamSpecString::builder("title").read_only().build(),
                    ParamSpecString::builder("artist").read_only().build(),
                    ParamSpecObject::builder::<glib::BoxedAnyObject>("release-date").read_only().build(),
                    ParamSpecString::builder("quality-grade").read_only().build()
                ]
            });
            PROPERTIES.as_ref()
        }

        fn property(&self, _id: usize, pspec: &ParamSpec) -> glib::Value {
            let obj = self.obj();
            match pspec.name() {
                "uri" => obj.get_uri().to_value(),
                "title" => obj.get_title().to_value(),
                "artist" => obj.get_artist_str().to_value(),
                "release-date" => glib::BoxedAnyObject::new(obj.get_release_date()).to_value(),
                "quality-grade" => obj.get_quality_grade().to_value(),
                _ => unimplemented!(),
            }
        }
    }
}

glib::wrapper! {
    pub struct Album(ObjectSubclass<imp::Album>);
}

impl Album {
    // ALL of the getters below require that the info field be initialised!
    pub fn get_info(&self) -> &AlbumInfo {
        &self.imp().info.get().unwrap()
    }

    pub fn get_uri(&self) -> &str {
        &self.get_info().uri
    }

    pub fn get_title(&self) -> &str {
        &self.get_info().title
    }

    pub fn get_artists(&self) -> &[ArtistInfo] {
        &self.get_info().artists
    }

    pub fn get_artist_str(&self) -> Option<String> {
        artists_to_string(&self.get_info().artists)
    }

    pub fn get_mbid(&self) -> Option<&str> {
        self.get_info().mbid.as_deref()
    }

    pub fn get_release_date(&self) -> Option<Date> {
        self.get_info().release_date.clone()
    }

    pub fn get_quality_grade(&self) -> QualityGrade {
        self.get_info().quality_grade.clone()
    }
}

impl Default for Album {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl From<AlbumInfo> for Album {
    fn from(info: AlbumInfo) -> Self {
        let res = glib::Object::builder::<Self>().build();
        res.imp().info.set(info);
        res
    }
}
