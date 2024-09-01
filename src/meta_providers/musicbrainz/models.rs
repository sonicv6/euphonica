use chrono::NaiveDate;
use musicbrainz_rs::{
    entity::{
        artist::{Artist, ArtistType, Gender},
        release::Release,
        tag::Tag
    }, prelude::*
};
use super::super::{
    models,
    prelude::*
};

impl From<Tag> for models::Tag {
    fn from(mbtag: Tag) -> Self {
        Self {
            name: mbtag.name,
            count: Some(mbtag.count),
            url: None
        }
    }
}

// TODO: l10n
pub fn mb_gender_to_str(g: Gender) -> Option<String> {
    match g {
        Gender::Male => Some(String::from("male")),
        Gender::Female => Some(String::from("female")),
        Gender::Other => Some(String::from("other")),
        _ => None  // Company/legal entity, etc.
    }
}

impl From<Release> for models::AlbumMeta {
    fn from(rel: Release) -> Self {
        // TODO: Keep more of the data in Release.
        let artist_tag: Option<String>;
        if let Some(artists) = rel.artist_credit {
            let mut content = String::new();
            for artist in artists.iter() {
                content.push_str(&artist.name);
                // Spaces should already be included.
                // Last artist should not have one.
                if let Some(delim) = &artist.joinphrase {
                    content.push_str(delim);
                }
            }
            artist_tag = Some(content);
        }
        else {
            artist_tag = None;
        }
        let tags: Vec<models::Tag>;
        if let Some(mbtags) = rel.tags {
            tags = mbtags.into_iter().map(models::Tag::from).collect();
        }
        else {
            tags = Vec::new();
        }

        Self {
            name: rel.title,
            artist: artist_tag,
            mbid: Some(rel.id.clone()),
            tags,
            image: Vec::new(), // acquired separately
            url: Some(format!("https://musicbrainz.org/release/{}", rel.id)),
            wiki: None // not provided
        }
    }
}


impl From<Artist> for models::ArtistMeta {
    fn from(artist: Artist) -> Self {
        // TODO: Keep more of the data in Artist.
        let tags: Vec<models::Tag>;
        if let Some(mbtags) = artist.tags {
            tags = mbtags.into_iter().map(models::Tag::from).collect();
        }
        else {
            tags = Vec::new();
        }
        let begin_date: Option<NaiveDate>;
        let end_date: Option<NaiveDate>;
        if let Some(lifespan) = artist.life_span {
            begin_date = lifespan.begin;
            if lifespan.ended.unwrap_or(false) {
                end_date = lifespan.end; // Might still be None
            }
            else {
                end_date = None;
            }
        }
        else {
            begin_date = None;
            end_date = None;
        }

        Self {
            name: artist.name,
            tags,
            mbid: Some(artist.id.clone()),
            similar: Vec::new(),
            image: Vec::new(),
            url: Some(format!("https://musicbrainz.org/artist/{}", artist.id)),
            bio: None,
            artist_type: artist.artist_type.unwrap_or(ArtistType::Other),
            gender: mb_gender_to_str(artist.gender.unwrap_or(Gender::NotApplicable)),
            begin_date,
            end_date,
            country: artist.country
        }
    }
}
