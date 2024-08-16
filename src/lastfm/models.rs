use serde::{Deserialize, Serialize, Deserializer, Serializer};

// Common structs
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Tag {
    pub url: String,
    pub name: String
}
// For some reason the taglist resides in a nested "tag" object.
#[derive(Serialize, Deserialize)]
struct TagsHelper {
    tag: Vec<Tag>,
}

fn deserialize_tags<'de, D>(deserializer: D) -> Result<Vec<Tag>, D::Error>
where
    D: Deserializer<'de>,
{
    let helper: TagsHelper = Deserialize::deserialize(deserializer)?;
    Ok(helper.tag)
}
fn serialize_tags<S>(tags: &Vec<Tag>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let helper = TagsHelper { tag: tags.clone() };
    helper.serialize(serializer)
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Image {
    pub size: String,
    #[serde(rename = "#text")]
    pub url: String
}

// Album
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Wiki {
    pub summary: String,
    pub content: String,
    #[serde(default)]
    _other: ()
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[allow(clippy::manual_non_exhaustive)]
pub struct Album {
    pub artist: String,
    // If queried using mbid, it won't be returned again
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mbid: Option<String>,
    #[serde(
        deserialize_with = "deserialize_tags",
        serialize_with = "serialize_tags"
    )]
    pub tags: Vec<Tag>,
    pub image: Vec<Image>,
    pub url: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wiki: Option<Wiki>,
    #[serde(default)]
    _other: ()
}
#[derive(Deserialize)]
pub struct AlbumResponse {
    pub album: Album,
}
