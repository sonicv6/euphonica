# Euphonia

An MPD frontend with delusions of grandeur.

## Features
- GTK4 Libadwaita UI for most MPD features, from basic things like playback controls, queue reordering and ReplayGain to things like output control, crossfade and MixRamp configuration
- Audio quality indicators (lossy, lossless, hi-res, DSD) for individual songs as well as albums & detailed format printout
- Browse your library by album or artist, with browsing by folder, genre and other criteria in the works
- Sort albums by name, AlbumArtist or release date (provided you have the tags)
- Asynchronous search for large collections
- Configurable multi-artist tag syntax, works with anything you throw at it (in other words, your artist tags can be pretty messy and Euphonia will still be able to correctly split them into individual artists for the Artist View)
- Performant album art fetching & display (cached with Stretto)
- Background blur effect
- Automatically fetch album arts & artist avatars from external sources (currently supports Last.fm and MusicBrainz)
- Album wikis & artist bios are supported too
- All externally-acquired metadata are cached locally & persisted on disk to avoid needless API calls
- Volume knob with dBFS readout support ('cuz why not?)
- Written in Rust so my dumb code can still be quick :)

## Build

Euphonia is developed on, and so far has only been tested on Arch Linux (btw).

1. Make sure you have these dependencies installed beforehand:
  - gtk4 >= 4.16
  - libadwaita >= 1.6
  - rustup >= 1.27
  - meson >= 1.5
  - mpd >= 0.21 (Euphonia relies on the new filter syntax)

2. Init build folder
  ```bash
  cd /path/to/where/you/cloned/this/repo/euphonia
  meson setup build --buildtype=release
  ```

3. Compile & install (will require root privileges)
  ```bash
  cd build
  meson install
  ```
Flatpak & AUR releases are also planned.

## TODO
- Password support
- Browse by folder
- Browse by genre
- Realtime lyrics fetching
- Library management operations such as tag editing (will require access to the files themselves) 
