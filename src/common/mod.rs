pub mod song;
pub mod album;
pub mod inode;
pub mod artist;
pub mod paintables;
pub mod marquee;

pub use song::{SongInfo, Song, QualityGrade};
pub use inode::{INodeType, INode};
pub use album::{AlbumInfo, Album};
pub use marquee::Marquee;
pub use artist::{
    ArtistInfo,
    Artist,
    parse_mb_artist_tag,
    artists_to_string
};
