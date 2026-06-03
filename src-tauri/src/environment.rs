use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq)]
pub enum Environment {
    Hyprland,
    Sway,
    KdeWayland,
    GnomeWayland,
    GenericWayland,
    X11,
}

pub fn detect_environment() -> Environment {
    if std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok() {
        return Environment::Hyprland;
    }
    if std::env::var("SWAYSOCK").is_ok() {
        return Environment::Sway;
    }
    if std::env::var("KDE_FULL_SESSION").is_ok() && is_wayland() {
        return Environment::KdeWayland;
    }
    if std::env::var("GNOME_DESKTOP_SESSION_ID").is_ok() && is_wayland() {
        return Environment::GnomeWayland;
    }
    if is_wayland() {
        return Environment::GenericWayland;
    }
    Environment::X11
}

pub fn is_wayland() -> bool {
    std::env::var("WAYLAND_DISPLAY").is_ok()
}

pub fn get_active_window(env: &Environment) -> (String, String) {
    match env {
        Environment::Hyprland => get_hyprland_window(),
        Environment::Sway => get_sway_window(),
        Environment::KdeWayland => get_kde_window(),
        Environment::GnomeWayland => get_gnome_window(),
        _ => (String::new(), String::new()),
    }
}

fn get_hyprland_window() -> (String, String) {
    let output = std::process::Command::new("hyprctl")
        .args(["activewindow", "-j"])
        .output();
    match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            if let Ok(data) = serde_json::from_str::<serde_json::Value>(&stdout) {
                let class = data.get("class").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let title = data.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
                return (class, title);
            }
        }
        _ => {}
    }
    (String::new(), String::new())
}

fn get_sway_window() -> (String, String) {
    let output = std::process::Command::new("swaymsg")
        .args(["-t", "get_tree"])
        .output();
    match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            if let Ok(tree) = serde_json::from_str::<serde_json::Value>(&stdout) {
                if let Some(focused) = find_focused_sway_node(&tree) {
                    let app_id = focused.get("app_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let name = focused.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    return (app_id, name);
                }
            }
        }
        _ => {}
    }
    (String::new(), String::new())
}

fn find_focused_sway_node(node: &serde_json::Value) -> Option<serde_json::Value> {
    if node.get("focused").and_then(|v| v.as_bool()) == Some(true) {
        return Some(node.clone());
    }
    if let Some(nodes) = node.get("nodes").and_then(|v| v.as_array()) {
        for child in nodes {
            if let Some(found) = find_focused_sway_node(child) {
                return Some(found);
            }
        }
    }
    if let Some(nodes) = node.get("floating_nodes").and_then(|v| v.as_array()) {
        for child in nodes {
            if let Some(found) = find_focused_sway_node(child) {
                return Some(found);
            }
        }
    }
    None
}

fn get_kde_window() -> (String, String) {
    let output = std::process::Command::new("kdotool")
        .args(["getactivewindow"])
        .output();
    match output {
        Ok(out) if out.status.success() => {
            let id_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !id_str.is_empty() {
                let info = std::process::Command::new("kdotool")
                    .args(["getwindowproperty", "--all", &id_str])
                    .output();
                if let Ok(info_out) = info {
                    let info_str = String::from_utf8_lossy(&info_out.stdout);
                    // Parse class and title from kdotool output
                    let class = extract_kde_property(&info_str, "class");
                    let title = extract_kde_property(&info_str, "title");
                    return (class, title);
                }
            }
        }
        _ => {}
    }
    (String::new(), String::new())
}

fn extract_kde_property(text: &str, key: &str) -> String {
    for line in text.lines() {
        if line.to_lowercase().contains(key) {
            if let Some(val) = line.split(':').nth(1) {
                return val.trim().trim_matches('"').to_string();
            }
        }
    }
    String::new()
}

fn get_gnome_window() -> (String, String) {
    let script = r#"
        const Shell = imports.gi.Shell;
        const global = Shell.Global.get();
        const win = global.get_window_actors().find(w => w.meta_window.has_focus());
        if (win) {
            print(win.meta_window.get_wm_class());
            print(win.meta_window.get_title());
        }
    "#;
    let output = std::process::Command::new("gdbus")
        .args([
            "call", "--session", "--dest", "org.gnome.Shell",
            "--object-path", "/org/gnome/Shell",
            "--method", "org.gnome.Shell.Eval",
            script,
        ])
        .output();
    match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let parts: Vec<&str> = stdout.lines().collect();
            if parts.len() >= 2 {
                return (parts[0].trim().to_string(), parts[1].trim().to_string());
            }
        }
        _ => {}
    }
    (String::new(), String::new())
}

pub static TERMINAL_CLASSES: once_cell::sync::Lazy<HashSet<&'static str>> =
    once_cell::sync::Lazy::new(|| {
        [
            "kitty", "alacritty", "foot", "wezterm", "gnome-terminal",
            "konsole", "xfce4-terminal", "urxvt", "urxvt-256color",
            "xterm", "xterm-256color", "st", "st-256color", "termite",
            "terminator", "tilix", "sakura", "lxterminal", "guake",
            "yakuake", "tilda", "cool-retro-term", "deepin-terminal",
            "kgx", "blackbox", "contour", "hyper", "pterm", "tabby",
            "terminal", "ptyxis", "ghostty", "rio", "mintty", "putty",
            "code-oss", "cursor", "opencode",
        ]
        .into_iter()
        .collect()
    });

pub fn needs_shift_paste(env: &Environment, window_class: &str) -> bool {
    if *env == Environment::X11 {
        return TERMINAL_CLASSES.contains(window_class);
    }
    // On Wayland, check if we're in a terminal
    TERMINAL_CLASSES.contains(window_class)
}
