use std::{
    hash::Hash,
    io::Cursor
};
use image::{
    DynamicImage,
    imageops::FilterType,
    io::Reader as ImageReader
};
use rustc_hash::FxHashSet;
use gtk::gio;
use gio::prelude::*;
use gtk::Ordering;
use crate::config::APPLICATION_ID;
use mpd::status::AudioFormat;

pub fn settings_manager() -> gio::Settings {
    // Trim the .Devel suffix if exists
    let app_id = APPLICATION_ID.trim_end_matches(".Devel");
    gio::Settings::new(app_id)
}

pub fn format_secs_as_duration(seconds: f64) -> String {
    let total_seconds = seconds.round() as i64;
    let days = total_seconds / 86400;
    let hours = (total_seconds % 86400) / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    if days > 0 {
        format!(
            "{} days {:02}:{:02}:{:02}",
            days, hours, minutes, seconds
        )
    } else if hours > 0 {
        format!(
            "{:02}:{:02}:{:02}",
            hours, minutes, seconds
        )
    } else {
        format!(
            "{:02}:{:02}",
            minutes, seconds
        )
    }
}

// For convenience
pub fn prettify_audio_format(format: &AudioFormat) -> String {
    // Here we need to re-infer whether this format is DSD or PCM
    // Only detect DSD64 at minimum, anything lower is too esoteric
    if format.bits == 1 && format.rate >= 352800 {
        // Is probably DSD
        let sample_rate = format.rate * 8;
        return format!(
            "{} ({:.4}MHz) {}ch",
            sample_rate / 44100,
            (sample_rate as f64) / 1e6,
            format.chans
        );
    }
    format!(
        "{}bit {:.1}kHz {}ch",
        format.bits,
        (format.rate as f64) / 1e3,
        format.chans
    )
}

pub fn g_cmp_options<T: Ord>(s1: Option<&T>, s2: Option<&T>, nulls_first: bool, asc: bool) -> Ordering {
    if s1.is_none() && s2.is_none() {
        return Ordering::Equal;
    }
    else if s1.is_none() {
        if nulls_first {
            return Ordering::Smaller;
        }
        return Ordering::Larger;
    }
    else if s2.is_none() {
        if nulls_first {
            return Ordering::Larger;
        }
        return Ordering::Smaller;
    }
    if asc {
        return Ordering::from(s1.unwrap().cmp(s2.unwrap()));
    }
    Ordering::from(s2.unwrap().cmp(s1.unwrap()))
}

pub fn g_cmp_str_options(
    s1: Option<&str>, s2: Option<&str>,
    nulls_first: bool, asc: bool,
    case_sensitive: bool
) -> Ordering {
    if s1.is_none() && s2.is_none() {
        return Ordering::Equal;
    }
    else if s1.is_none() {
        if nulls_first {
            return Ordering::Smaller;
        }
        return Ordering::Larger;
    }
    else if s2.is_none() {
        if nulls_first {
            return Ordering::Larger;
        }
        return Ordering::Smaller;
    }
    if asc {
        if case_sensitive {
            return Ordering::from(s1.unwrap().cmp(s2.unwrap()));
        }
        return Ordering::from(
            s1.unwrap().to_lowercase().cmp(
                &s2.unwrap().to_lowercase()
            )
        );
    }
    if case_sensitive {
        return Ordering::from(s2.unwrap().cmp(s1.unwrap()));
    }
    Ordering::from(
        s2.unwrap().to_lowercase().cmp(
            &s1.unwrap().to_lowercase()
        )
    )
}

pub fn g_search_substr(
    text: Option<&str>, term: &str,
    case_sensitive: bool
) -> bool {
    if text.is_none() && term.is_empty() {
        return true;
    }
    else if text.is_some() && !term.is_empty() {
        if case_sensitive {
            return text.unwrap().contains(term);
        }
        return text.unwrap().to_lowercase().contains(
            &term.to_lowercase()
        );
    }
    false
}


pub fn strip_filename_linux(path: &str) -> &str {
    // MPD insists on having a trailing slash so here we go
    if let Some(last_slash) = path.rfind('/') {
        return &path[..last_slash + 1];
    }
    path
}

pub fn read_image_from_bytes(bytes: Vec<u8>) -> Option<DynamicImage> {
    if let Ok(reader) = ImageReader::new(Cursor::new(bytes)).with_guessed_format() {
        if let Ok(dyn_img) = reader.decode() {
            return Some(dyn_img);
        }
        return None;
    }
    None
}

/// Automatically resize image based on user settings.
/// All providers should use this function on their child threads to resize applicable images
/// before returning the images to the main thread.
/// Two images will be returned: a high-resolution version and a thumbnail version.
/// Their major axis's resolution is determined by the keys hires-image-size and
/// thumbnail-image-size in the gschema respectively.
pub fn resize_image(dyn_img: DynamicImage) -> (DynamicImage, DynamicImage) {
    let settings = settings_manager().child("library");
    let hires_size = settings.uint("hires-image-size");
    let thumbnail_size = settings.uint("thumbnail-image-size");
    (
        dyn_img.resize(hires_size, hires_size, FilterType::CatmullRom),
        dyn_img.thumbnail(thumbnail_size, thumbnail_size)
    )
}

// TODO: Optimise this
pub fn deduplicate<T: Eq + Hash + Clone>(input: &[T]) -> Vec<T> {
    let mut seen = FxHashSet::default();
    input.iter().filter(|item| seen.insert(item.clone())).cloned().collect()
}
