use serde::{Deserialize, Serialize, Deserializer, Serializer};

// Common structs
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Tag {
    pub url: String,
    pub name: String
}
// For some reason the taglist resides in a nested "tag" object.
#[derive(Serialize, Deserialize)]
struct NestedTagList {
    tag: Vec<Tag>,
}
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum TagsHelper {
    String(String),
    Nested(NestedTagList)
}

fn deserialize_tags<'de, D>(deserializer: D) -> Result<Vec<Tag>, D::Error>
where
    D: Deserializer<'de>,
{
    let helper: TagsHelper = Deserialize::deserialize(deserializer)?;
    match helper {
        TagsHelper::String(_) => Ok(Vec::with_capacity(0)),
        TagsHelper::Nested(nested) => Ok(nested.tag)
    }
}
fn serialize_tags<S>(tags: &[Tag], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    if tags.is_empty() {
        let helper = TagsHelper::String("".to_owned());
        helper.serialize(serializer)
    }
    else {
        let helper = TagsHelper::Nested(
            NestedTagList {
                tag: tags.to_owned()
            }
        );
        helper.serialize(serializer)
    }

}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Image {
    pub size: String,
    #[serde(rename = "#text")]
    pub url: String
}

// Album
#[derive(Serialize, Deserialize, Debug, Clone)]
#[non_exhaustive]
pub struct Wiki {
    pub summary: String,
    pub content: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[non_exhaustive]
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
}
#[derive(Deserialize)]
pub struct AlbumResponse {
    pub album: Album,
}
