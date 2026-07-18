use crate::manifest::{find_installed_bp_names, find_user_manifest};
use ratatui::{
    layout::{Constraint, Flex, Layout, Rect},
    widgets::ListState,
};

/// Advance or retreat a wrapping index within `0..count`.
pub(crate) fn wrapping_nav(index: &mut usize, count: usize, forward: bool) {
    if count > 0 {
        *index = if forward {
            (*index + 1) % count
        } else {
            (*index + count - 1) % count
        };
    }
}

/// Clamped (non-wrapping) movement on a `ListState` within `0..count`.
pub(crate) fn list_nav(state: &mut ListState, count: usize, forward: bool) {
    if let Some(selected) = state.selected() {
        if forward {
            if selected < count.saturating_sub(1) {
                state.select(Some(selected + 1));
            }
        } else if selected > 0 {
            state.select(Some(selected - 1));
        }
    }
}

pub(crate) fn wait_for_enter() {
    // ratatui::restore() leaves the alternate screen and disables raw mode but
    // does not re-show the cursor, so we do it explicitly here.
    let _ = crossterm::execute!(std::io::stdout(), crossterm::cursor::Show);
    println!("\nPress Enter to return to TUI...");
    let _ = std::io::stdin().read_line(&mut String::new());
}

/// Detect whether we're inside a Cargo project and which battery packs are installed.
pub(crate) fn detect_project_state() -> (bool, Vec<String>) {
    let Ok(project_dir) = std::env::current_dir() else {
        return (false, Vec::new());
    };
    let Ok(manifest_path) = find_user_manifest(&project_dir) else {
        return (false, Vec::new());
    };
    let Ok(content) = std::fs::read_to_string(&manifest_path) else {
        return (true, Vec::new());
    };
    let names = find_installed_bp_names(&content).unwrap_or_default();
    (true, names)
}

pub(crate) fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Percentage(percent_y)]).flex(Flex::Center);
    let horizontal = Layout::horizontal([Constraint::Percentage(percent_x)]).flex(Flex::Center);
    let [v_area] = vertical.areas(area);
    let [h_area] = horizontal.areas(v_area);
    h_area
}

/// Build a GitHub tree URL for a directory path.
/// If repository is set and looks like a GitHub URL, construct a tree URL.
/// Otherwise fall back to a crates.io search or just open the repo root.
/// Build a GitHub blob URL for a file path.
pub(crate) fn build_github_blob_url(repository: Option<&str>, path: &str) -> String {
    build_github_ref_url(repository, "blob", path)
}

/// Build a GitHub URL with the specified ref type (tree or blob).
pub(crate) fn build_github_ref_url(repository: Option<&str>, ref_type: &str, path: &str) -> String {
    match repository {
        Some(repo) => {
            // Try to parse GitHub URL: https://github.com/owner/repo
            if let Some(gh_path) = repo
                .strip_prefix("https://github.com/")
                .or_else(|| repo.strip_prefix("http://github.com/"))
            {
                // Remove trailing .git if present
                let gh_path = gh_path.strip_suffix(".git").unwrap_or(gh_path);
                // Remove trailing slash
                let gh_path = gh_path.trim_end_matches('/');
                // Construct URL with main branch
                format!("https://github.com/{}/{}/main/{}", gh_path, ref_type, path)
            } else {
                // Not a GitHub URL, just open the repository URL
                repo.to_string()
            }
        }
        None => {
            // No repository, can't construct URL
            // Fall back to nothing useful - this shouldn't happen in practice
            "https://crates.io".to_string()
        }
    }
}
