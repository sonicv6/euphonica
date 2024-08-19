use regex::Regex;
use std::cell::RefCell;
use time::Date;
use gtk::glib;
use gtk::gdk::Texture;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

use crate::utils::strip_filename_linux;
use super::{Song, QualityGrade};

/// Artist struct, for use with both Artist and AlbumArtist tags.
#[derive(Debug, Clone)]
pub struct ArtistInfo {
    // TODO: Might want to refactor to Into<Cow<'a, str>>
    pub name: String, // Artist tag, not AlbumArtist
    pub avatar: Option<Texture>,
    pub mbid: Option<String>,
    pub is_composer: bool
}

impl ArtistInfo {
    pub fn new(name: &str, is_composer: bool) -> Self {
        Self {
            name: name.to_owned(),
            avatar: None,
            mbid: None,
            is_composer
        }
    }
}

impl Default for ArtistInfo {
    fn default() -> Self {
        ArtistInfo {
            name: "Untitled Artist".to_owned(),
            avatar: None,
            mbid: None,
            is_composer: false
        }
    }
}

/// Utility function to create a list of ArtistInfo objects from a MusicBrainz Artist tag.
/// Can be used with AlbumArtist tag too, but NOT with with ArtistSort or AlbumArtistSort tags.
pub fn parse_mb_artist_tag(tag: &str) -> Vec<ArtistInfo> {
    let re = Regex::new(r"([^,;]+)([,:]?)").unwrap();
    let mut res = Vec::new();

    for cap in re.captures_iter(tag) {
        let name = cap[1].trim();
        let maybe_sep = cap.get(2).map(|s| s.as_str());
        let is_composer: bool;
        if let Some(sep) = maybe_sep {
            is_composer = sep == ";"
        }
        else {
            // No idea, might be one though
            is_composer = false;
        }
        res.push(ArtistInfo::new(name, is_composer));
    }

    res
}

pub fn artists_to_string(artists: &[ArtistInfo]) -> Option<String> {
    if artists.is_empty() {
        None
    }
    else if artists.len() > 1 {
        // For now assume that only the first artist in the list can be a composer
        let mut res: String = "".to_owned();
        for (i, artist) in artists.iter().enumerate() {
            if i > 0 {
                let sep = if artists[i - 1].is_composer {
                    "; "
                } else {
                    ", "
                };
                res.push_str(sep);
            }
            res.push_str(artist.name.as_ref());
        }
        Some(res)
    }
    else {
        Some(artists[0].name.clone())
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
    pub struct Artist {
        pub info: RefCell<ArtistInfo>
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Artist {
        const NAME: &'static str = "EuphoniaArtist";
        type Type = super::Artist;

        fn new() -> Self {
            Self {
                info: RefCell::new(ArtistInfo::default())
            }
        }
    }

    impl ObjectImpl for Artist {
        fn properties() -> &'static [ParamSpec] {
            static PROPERTIES: Lazy<Vec<ParamSpec>> = Lazy::new(|| {
                vec![
                    ParamSpecString::builder("name")
                        .read_only()
                        .build(),
                    ParamSpecObject::builder::<Texture>("avatar")
                        .read_only()
                        .build(),
                ]
            });
            PROPERTIES.as_ref()
        }

        fn property(&self, _id: usize, pspec: &ParamSpec) -> glib::Value {
            let obj = self.obj();
            match pspec.name() {
                "name" => obj.get_name().to_value(),
                "avatar" => obj.get_avatar().to_value(),
                _ => unimplemented!(),
            }
        }
    }
}

glib::wrapper! {
    pub struct Artist(ObjectSubclass<imp::Artist>);
}

impl Artist {
    pub fn get_info(&self) -> ArtistInfo {
        self.imp().info.borrow().clone()
    }

    pub fn get_name(&self) -> String {
        self.imp().info.borrow().name.clone()
    }

    pub fn get_mbid(&self) -> Option<String> {
        self.imp().info.borrow().mbid.clone()
    }

    pub fn get_avatar(&self) -> Option<Texture> {
        self.imp().info.borrow().avatar.clone()
    }

    pub fn set_avatar(&self, maybe_tex: Option<Texture>) {
        if let Some(tex) = maybe_tex {
            self.imp().info.borrow_mut().avatar.replace(tex);
        }
        else {
            let _ = self.imp().info.borrow_mut().avatar.take();
        }
        self.notify("avatar");
    }

    pub fn is_composer(&self) -> bool {
        self.imp().info.borrow().is_composer
    }
}

impl Default for Artist {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl From<ArtistInfo> for Artist {
    fn from(info: ArtistInfo) -> Self {
        let res = glib::Object::builder::<Self>().build();
        res.imp().info.replace(info);
        res
    }
}
