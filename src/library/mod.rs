mod album_view;
mod album_cell;
mod album_content_view;
mod album_song_row;

mod artist_view;
mod artist_cell;
mod artist_content_view;
mod artist_song_row;
mod playlist_song_row;

mod folder_view;

mod playlist_view;
mod playlist_content_view;

// Common stuff shared between views
mod generic_row;
mod add_to_playlist;

// The Library controller itself
mod controller;

pub use album_view::AlbumView;
use album_cell::AlbumCell;
pub use album_content_view::AlbumContentView;
use album_song_row::AlbumSongRow;

pub use artist_view::ArtistView;
use artist_cell::ArtistCell;
use artist_song_row::ArtistSongRow;
pub use artist_content_view::ArtistContentView;

pub use folder_view::FolderView;

pub use playlist_view::PlaylistView;
pub use playlist_content_view::PlaylistContentView;
pub use playlist_song_row::PlaylistSongRow;

pub use controller::Library;
