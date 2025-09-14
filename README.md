![Euphonica icon](data/icons/hicolor/scalable/apps/io.github.htkhiem.Euphonica.svg)
# Euphonica

![Flathub Downloads](https://img.shields.io/flathub/downloads/io.github.htkhiem.Euphonica?style=flat-square&logo=flathub&link=https%3A%2F%2Fflathub.org%2Fen%2Fapps%2Fio.github.htkhiem.Euphonica)

An MPD frontend with delusions of grandeur. 

It exists to sate my need for something that's got the bling and the features to back that bling up.

## Features
- Responsive GTK4 LibAdwaita UI for most MPD features, from basic things like playback controls, queue reordering and ReplayGain to things like output control, crossfade and MixRamp configuration
- Automatically fetch album arts, artist avatars and (synced) song lyrics from external sources (currently supports Last.fm, MusicBrainz and LRCLIB). All externally-acquired metadata are cached locally & persisted on disk to avoid needless API calls.
- Built-in, customisable spectrum visualiser, reading from MPD FIFO or system PipeWire
- Automatic accent colours based on album art (optional)
- Integrated MPRIS client with background run supported. The background instance can be reopened via your shell's MPRIS applet, the "Background applications" section in GNOME's quick settings shade (if installed via Flatpak) or simply by launching Euphonica again.
- Rate albums (requires MPD 0.24+)
- Audio quality indicators (lossy, lossless, hi-res, DSD) for individual songs as well as albums & detailed format printout
- Browse your library by album, artist and folders with multiselection support
  - Browsing by genre and other criteria are planned.
- Playlist browser and editor:
  - Save current queue as playlist
  - Create playlists from selected songs or append to existing ones
  - Rename existing playlists + reorder or remove songs in them
- Sort albums by name, AlbumArtist or release date (provided you have the tags)
- Asynchronous search for large collections
- Configurable multi-artist tag syntax, works with anything you throw at it
  - In other words, your artist tags can be pretty messy and Euphonica will still be able to correctly split them into individual artists.
- Performant album art fetching & display (cached with Stretto)
- Super-fast, **multithreaded**, **statically-cached** background blur powered by [libblur](https://github.com/awxkee/libblur)'s stack blur implementation.
  - Completely independent of blur radius in terms of time complexity.
  - Multithreaded, queued update logic never blocks UI and **only runs when needed** (once _after_ window resizes, once every time album art changes, etc). 
- Volume knob with dBFS readout support ('cuz why not?)
- User-friendly configuration UI & GSettings backend
- MPD passwords are securely stored in your user's login keyring
- Commands are bundled into lists for efficient MPD-side processing where possible.
- Written in Rust so my dumb code can still be quick :)

## Screenshots

The below were captured with a mix of dark and light modes.

- Recent View[^1]
  <img width="1122" height="822" alt="Screenshot From 2025-07-23 20-36-09" src="https://github.com/user-attachments/assets/026e0fcf-2988-4b26-8407-4d29e03b99e5" />

- Album View[^1]
  <img width="1122" height="822" alt="Screenshot From 2025-07-23 20-49-49" src="https://github.com/user-attachments/assets/df429e18-fba2-421a-b114-3dd40dbb0c0c" />

- UI at different sizes (v0.12+)[^1]
  ![mini-layouts-v2](https://github.com/user-attachments/assets/b41f5b50-013f-4c3e-952c-4e858f4cc1fa)

- Queue View[^1]
  ![queue-view](https://github.com/user-attachments/assets/b4d213db-13c0-4a33-85b6-cdf227c93d61)

- Visualiser & synced lyrics in action
  ![lyrics-animation](https://github.com/user-attachments/assets/1de89a90-90bd-4aa2-9775-5c1845d4dcf4)
  
- Artist bio as fetched from Last.fm[^1][^2][^3]
  ![artist-content-view](https://github.com/user-attachments/assets/54161399-1f16-490f-91b9-89b581b28839)

- Album wiki as fetched from Last.fm[^1][^2]
  <img width="1122" height="822" alt="Screenshot From 2025-07-23 20-46-02" src="https://github.com/user-attachments/assets/6b030124-7c46-4ac5-8558-d323d3a40d12" />
  
- Playlist Content View[^1]
  ![playlist-content-view](https://github.com/user-attachments/assets/be9913e7-2378-4374-9a8a-d08512fc1e09)
  
- Settings GUI for pretty much everything[^1]
  ![visualiser-customisation](https://github.com/user-attachments/assets/baed1ece-be17-4f39-81b3-df17e1460417)
  ![image](https://github.com/user-attachments/assets/f1277d5c-d0c4-40c0-81e2-201c581d4e44)


[^1]: Actual album arts and artist images have been replaced with random pictures from [Pexels](https://www.pexels.com/). All credits go to the original authors.
[^2]: Artist bios and album wikis are user-contributed and licensed by Last.fm under CC-BY-SA.
[^3]: The displayed image has been released into the public domain. More information at [Wikimedia Commons](https://commons.wikimedia.org/wiki/File:Johann_Sebastian_Bach.jpg).

## Installation

Euphonica is still in very early development, and so far has only been tested on Arch Linux (btw).

The preferred way to install Euphonica is as a Flatpak app via Flathub:

<a href='https://flathub.org/apps/io.github.htkhiem.Euphonica'>
    <img width='240' alt='Get it on Flathub' src='https://flathub.org/api/badge?locale=en'/>
</a>

Other ways to install Euphonica are listed below:

<details>
  <summary><h3>Arch Linux</h3></summary>
  
  An (admittedly experimental) AUR package is [now available](https://aur.archlinux.org/packages/euphonica-git).

  ```bash
  # Use your favourite AUR helper here
  paru -S euphonica-git
  ```
  
</details>

<details>
  <summary><h3>Nixpkgs</h3></summary>

  The Nix package is kindly maintained by [@paperdigits](https://github.com/paperdigits) [here](https://search.nixos.org/packages?channel=unstable&show=euphonica&from=0&size=50&sort=relevance&type=packages).

  ```bash
  # NixOS configuration
  environment.systemPackages = [
    pkgs.euphonica
  ];
  
  # For standalone Nix, without flakes:
  nix-env -iA nixpkgs.euphonica
  # With flakes:
  nix profile install nixpkgs#euphonica
  ```
</details>

## Set-up

Euphonica requires some preparation before it can be used, especially if you have never used an MPD client before. Please see the [wiki article](https://github.com/htkhiem/euphonica/wiki/Installation-&-basic-local-configuration) for instructions on setting up a basic local instance.

## Build

<details>
  <summary><h3>Flatpak</h3></summary>

  Euphonica can also be built from source using `flatpak-builder`.
  
  This builds and installs Euphonica as a sandboxed Flatpak app on your system, complete with an entry in 
  Flatpak-aware app stores (like GNOME Software, KDE Discover, etc). It should also work on virtually any 
  distribution, and does not require root privileges. Unlike installing from Flathub, this always builds
  the **latest** commit and as such is more suitable for development and testing purposes. Also, it might
  **overwrite** the existing Flathub-distributed installation in case you have one.
  
  1. Add the Flathub repo in case you haven't already:
     
  ```bash
  flatpak remote-add --user --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo
  ```
  2. Run `flatpak-builder` as follows:
     
  ```bash
  cd /path/to/where/you/cloned/euphonica
  flatpak-builder --force-clean --user --install-deps-from=flathub --repo=repo --install build-flatpak io.github.htkhiem.Euphonica-dev.json
  ```
  3. Once the above has completed, you can run Euphonica using:
  
  ```bash
  flatpak run io.github.htkhiem.Euphonica
  ```
  
  
  A desktop icon entry should also have been installed for you, although it might take a reboot to show up.
  
</details>

<details>
  <summary><h3>Meson</h3></summary>

  This builds Euphonica against system library packages, then installs it directly into `/usr/local/bin`.
  It is the most lightweight option, but has only been tested on Arch Linux.
  
  1. Make sure you have these dependencies installed beforehand:
    - `gtk4` >= 4.18
    - `libadwaita` >= 1.7
    - `meson` >= 1.5
    - `gettext` >= 0.23
    - `mpd` >= 0.24 (Euphonica relies on the new filter syntax and expanded tagging)
    - `sqlite` (metadata store dependency)
    - An `xdg-desktop-portal` provider
    - The latest stable Rust toolchain. I highly recommend using `rustup` to manage them. Using it, you can install the latest stable toolchain using `rustup default stable` or update your existing one with `rustup update`. Ensure that `rustc` and `cargo` are of at least version `1.88.0`.
    
      If you are on Arch Linux, `gettext` should have been installed as part of the `base-devel` metapackage, which also includes `git` (to clone this repo :) ).
  
  2. Init build folder
   
  ```bash
  cd /path/to/where/to/clone/euphonica
  git clone https://github.com/htkhiem/euphonica.git
  cd euphonica
  git submodule update --init
  meson setup build --buildtype=release
  ```

  3. Compile & install (will require root privileges)
     
  ```bash
  cd build
  meson install
  ```
</details>

## TODO
- Support more stickers-based features:
  - Recently played
  - Per-song ratings
  - User-editable album wikis and artist bios
  - Metadata sync between Euphonica instances (instead of being stored locally)
  - Should follow existing sticker schemas, such as that proposed by myMPD, where possible.
- Local socket-exclusive features:
  - Library management operations such as tag editing (will require access to the files themselves)
  - Save downloaded album arts and artist avatars directly into the music folders themselves so other instances
    and clients can use them.
- Browse by genre
- An "All tracks" page with advanced, freeform querying to take full advantage of MPD v0.21+'s new query syntax
