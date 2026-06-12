pub mod app;
pub mod monitor_arrange;
pub mod profile_editor;
pub mod profile_list;
mod styles;
pub mod thunderbolt;

pub use app::App;

/// Truncate a string to at most `max` characters, appending "..." if cut.
/// Char-based (not byte-based) so multi-byte EDID strings can't panic.
fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        let truncated: String = s.chars().take(max).collect();
        format!("{}...", truncated)
    } else {
        s.to_string()
    }
}
