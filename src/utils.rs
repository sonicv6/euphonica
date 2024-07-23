use gtk::gio;
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
        return format!(
            "{} days {:02}:{:02}:{:02}",
            days, hours, minutes, seconds
        );
    } else if hours > 0 {
        return format!(
            "{:02}:{:02}:{:02}",
            hours, minutes, seconds
        );
    } else {
        return format!(
            "{:02}:{:02}",
            minutes, seconds
        );
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
