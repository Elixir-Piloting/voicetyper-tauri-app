use crate::environment::{self, Environment};

pub fn copy_to_clipboard(env: &Environment, text: &str) -> Result<(), String> {
    match env {
        Environment::Hyprland
        | Environment::Sway
        | Environment::KdeWayland
        | Environment::GnomeWayland
        | Environment::GenericWayland => wl_copy(text),
        Environment::X11 => xclip(text),
    }
}

pub fn simulate_paste(env: &Environment, window_class: &str) -> Result<(), String> {
    let shift = environment::needs_shift_paste(env, window_class);
    match env {
        Environment::Hyprland
        | Environment::Sway
        | Environment::KdeWayland
        | Environment::GnomeWayland
        | Environment::GenericWayland => wtype_paste(shift),
        Environment::X11 => xdotool_paste(shift),
    }
}

fn wl_copy(text: &str) -> Result<(), String> {
    let mut child = std::process::Command::new("wl-copy")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("wl-copy spawn: {}", e))?;

    use std::io::Write;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(text.as_bytes())
            .map_err(|e| format!("wl-copy write: {}", e))?;
    }

    let status = child
        .wait()
        .map_err(|e| format!("wl-copy wait: {}", e))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("wl-copy exit: {:?}", status.code()))
    }
}

fn xclip(text: &str) -> Result<(), String> {
    let mut child = std::process::Command::new("xclip")
        .args(["-selection", "clipboard"])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("xclip spawn: {}", e))?;

    use std::io::Write;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(text.as_bytes())
            .map_err(|e| format!("xclip write: {}", e))?;
    }

    let status = child
        .wait()
        .map_err(|e| format!("xclip wait: {}", e))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("xclip exit: {:?}", status.code()))
    }
}

fn wtype_paste(shift: bool) -> Result<(), String> {
    let mut cmd = std::process::Command::new("wtype");
    if shift {
        cmd.args(["-M", "ctrl", "-M", "shift", "v", "-m", "shift", "-m", "ctrl"]);
    } else {
        cmd.args(["-M", "ctrl", "v", "-m", "ctrl"]);
    }
    let status = cmd.status().map_err(|e| format!("wtype spawn: {}", e))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("wtype exit: {:?}", status.code()))
    }
}

fn xdotool_paste(shift: bool) -> Result<(), String> {
    let keys = if shift {
        "ctrl+shift+v"
    } else {
        "ctrl+v"
    };
    let status = std::process::Command::new("xdotool")
        .args(["key", "--clearmodifiers", keys])
        .status()
        .map_err(|e| format!("xdotool spawn: {}", e))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("xdotool exit: {:?}", status.code()))
    }
}
