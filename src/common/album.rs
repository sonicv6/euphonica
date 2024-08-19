use std::cell::RefCell;
use time::Date;
use gtk::glib;
use gtk::gdk::Texture;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

use crate::utils::strip_filename_linux;
use super::{
    Song,
    QualityGrade,
    ArtistInfo,
    parse_mb_artist_tag,
    artists_to_string
};

// This is a model class for queue view displays.
// It does not contain any actual song in terms of data.

#[derive(Debug, Clone)]
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

    #[derive(Default, Debug)]
    pub struct Album {
        pub info: RefCell<AlbumInfo>
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Album {
        const NAME: &'static str = "EuphoniaAlbum";
        type Type = super::Album;

        fn new() -> Self {
            Self {
                info: RefCell::new(AlbumInfo::default())
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
                    ParamSpecString::builder("quality-grade").read_only().build(),
                    ParamSpecObject::builder::<Texture>("cover")
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
                "title" => obj.get_title().to_value(),
                "artist" => obj.get_artist_str().to_value(),
                "release-date" => glib::BoxedAnyObject::new(obj.get_release_date()).to_value(),
                "quality-grade" => obj.get_quality_grade().to_value(),
                "cover" => obj.get_cover().to_value(),
                _ => unimplemented!(),
            }
        }
    }
}

glib::wrapper! {
    pub struct Album(ObjectSubclass<imp::Album>);
}

impl Album {
    pub fn get_info(&self) -> AlbumInfo {
        self.imp().info.borrow().clone()
    }

    pub fn get_uri(&self) -> String {
        self.imp().info.borrow().uri.clone()
    }

    pub fn get_title(&self) -> String {
        self.imp().info.borrow().title.clone()
    }

    pub fn get_artists(&self) -> Vec<ArtistInfo> {
        self.imp().info.borrow().artists.clone()
    }

    pub fn get_artist_str(&self) -> Option<String> {
        artists_to_string(&self.imp().info.borrow().artists)
    }

    pub fn get_mbid(&self) -> Option<String> {
        self.imp().info.borrow().mbid.clone()
    }

    pub fn get_cover(&self) -> Option<Texture> {
        self.imp().info.borrow().cover.clone()
    }

    pub fn get_release_date(&self) -> Option<Date> {
        self.imp().info.borrow().release_date.clone()
    }

    pub fn get_quality_grade(&self) -> QualityGrade {
        self.imp().info.borrow().quality_grade
    }

    pub fn set_cover(&self, maybe_tex: Option<Texture>) {
        if let Some(tex) = maybe_tex {
            self.imp().info.borrow_mut().cover.replace(tex);
        }
        else {
            let _ = self.imp().info.borrow_mut().cover.take();
        }
        self.notify("cover");
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
        res.imp().info.replace(info);
        res
    }
}
