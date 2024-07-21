use gtk::gio;
use crate::config::APPLICATION_ID;

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
