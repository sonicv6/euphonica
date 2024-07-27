use std::cell::RefCell;
use time::Date;
use gtk::glib;
use gtk::gdk::Texture;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

use crate::utils::strip_filename_linux;
use super::{Song, QualityGrade};

// This is a model class for queue view displays.
// It does not contain any actual song in terms of data.

#[derive(Debug, Clone)]
pub struct AlbumInfo {
    // TODO: Might want to refactor to Into<Cow<'a, str>>
    title: String,
    // Folder-based URI, acquired from the first song found with this album's tag.
    uri: String,
    artist: Option<String>,  // use AlbumArtist tag
    cover: Option<Texture>,
    release_date: Option<Date>,
    quality_grade: QualityGrade
}

impl AlbumInfo {
    pub fn new(uri: &str, title: &str, artist: Option<&str>) -> Self {
        Self {
            uri: uri.to_owned(),
            artist: artist.map(str::to_string),
            title: title.to_owned(),
            cover: None,
            release_date: None,
            quality_grade: QualityGrade::Unknown
        }
    }

    // copying all the strings instead of returning references.
    // This should allow for an easier ID3 tag editor implementation.
    pub fn title(&self) -> String {
        self.title.clone()
    }

    pub fn artist(&self) -> Option<String> {
        self.artist.clone()
    }

    pub fn uri(&self) -> String {
        self.uri.clone()
    }

    pub fn cover(&self) -> Option<Texture> {
        self.cover.clone()
    }
}

impl Default for AlbumInfo {
    fn default() -> Self {
        AlbumInfo {
            title: "Untitled Album".to_owned(),
            uri: "".to_owned(),
            artist: None,
            cover: None,
            release_date: None,
            quality_grade: QualityGrade::Unknown
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
                    ParamSpecString::builder("uri").construct_only().build(),
                    ParamSpecString::builder("title").build(),
                    ParamSpecString::builder("artist").build(),
                    ParamSpecObject::builder::<glib::BoxedAnyObject>("release-date").build(),
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
                "artist" => obj.get_artist().to_value(),
                "release-date" => glib::BoxedAnyObject::new(obj.get_release_date()).to_value(),
                "quality-grade" => obj.get_quality_grade().to_value(),
                "cover" => obj.get_cover().to_value(),
                _ => unimplemented!(),
            }
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &ParamSpec) {
            let obj = self.obj();
            match pspec.name() {
                "uri" => {
                    if let Ok(uri) = value.get::<&str>() {
                        uri.clone_into(&mut self.info.borrow_mut().uri);
                    }
                    obj.notify("uri");
                }
                "title" => {
                    if let Ok(title) = value.get::<&str>() {
                        title.clone_into(&mut self.info.borrow_mut().title);
                    }
                    obj.notify("title");
                }
                "artist" => {
                    if let Ok(artist) = value.get::<&str>() {
                        self.info.borrow_mut().artist.replace(artist.to_owned());
                    }
                    obj.notify("artist");
                }
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
        self.imp().info.borrow().uri()
    }

    pub fn get_title(&self) -> String {
        self.imp().info.borrow().title()
    }

    pub fn get_artist(&self) -> Option<String> {
        self.imp().info.borrow().artist()
    }

    pub fn get_cover(&self) -> Option<Texture> {
        self.imp().info.borrow().cover()
    }

    pub fn get_release_date(&self) -> Option<Date> {
        self.imp().info.borrow().release_date
    }

    pub fn get_quality_grade(&self) -> QualityGrade {
        self.imp().info.borrow().quality_grade
    }

    pub fn set_cover(&self, maybe_tex: Option<Texture>) {
        if let Some(tex) = maybe_tex {
            self.imp().info.borrow_mut().cover.replace(tex);
        }
        else {
            let _ = self.imp().info.borrow_mut().cover().take();
        }
        self.notify("cover");
    }
}

impl Default for Album {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl From<Song> for AlbumInfo {
    fn from(song: Song) -> Self {
        Self {
            title: song.get_album().unwrap_or("".to_owned()),
            uri: strip_filename_linux(&song.get_uri()).to_owned(),
            artist: song.get_album_artist(),
            cover: None,
            release_date: song.get_release_date(),
            quality_grade: song.get_quality_grade()
        }
    }
}

impl From<Song> for Album {
    fn from(song: Song) -> Self {
        let res = glib::Object::builder::<Self>().build();
        let info = AlbumInfo::from(song);
        res.imp().info.replace(info);
        res
    }
}

impl From<AlbumInfo> for Album {
    fn from(info: AlbumInfo) -> Self {
        let res = glib::Object::builder::<Self>().build();
        res.imp().info.replace(info);
        res
    }
}
