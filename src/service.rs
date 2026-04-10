use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use std::process::Command;

const SERVICE_NAME: &str = "gtkwallpapers";

fn unit_dir() -> Result<PathBuf> {
    let dir = dirs::config_dir()
        .context("could not resolve $HOME/.config")?
        .join("systemd")
        .join("user");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn unit_path() -> Result<PathBuf> {
    Ok(unit_dir()?.join(format!("{SERVICE_NAME}.service")))
}

fn binary_path() -> Result<String> {
    let exe = std::env::current_exe().context("could not determine binary path")?;
    Ok(exe.to_string_lossy().into_owned())
}

fn unit_contents() -> Result<String> {
    let bin = binary_path()?;
    Ok(format!(
        "[Unit]\n\
         Description=gtkwallpapers rotating wallpaper daemon\n\
         After=graphical-session.target\n\
         \n\
         [Service]\n\
         ExecStart={bin} daemon\n\
         Restart=on-failure\n\
         RestartSec=30\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n"
    ))
}

fn systemctl(args: &[&str]) -> Result<std::process::Output> {
    Command::new("systemctl")
        .arg("--user")
        .args(args)
        .output()
        .context("failed to run systemctl")
}

pub fn start() -> Result<()> {
    let path = unit_path()?;

    if !path.exists() {
        std::fs::write(&path, unit_contents()?)?;
        println!("Installed service unit: {}", path.display());
        systemctl(&["daemon-reload"])?;
        systemctl(&["enable", SERVICE_NAME])?;
    }

    let out = systemctl(&["start", SERVICE_NAME])?;
    if !out.status.success() {
        bail!(
            "systemctl start failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    println!("Service started.");
    Ok(())
}

pub fn stop() -> Result<()> {
    let out = systemctl(&["stop", SERVICE_NAME])?;
    if !out.status.success() {
        bail!(
            "systemctl stop failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    println!("Service stopped.");
    Ok(())
}

pub fn status() -> Result<()> {
    let out = systemctl(&["status", SERVICE_NAME])?;
    print!("{}", String::from_utf8_lossy(&out.stdout));
    Ok(())
}

pub fn uninstall() -> Result<()> {
    let _ = systemctl(&["stop", SERVICE_NAME]);
    let _ = systemctl(&["disable", SERVICE_NAME]);

    let path = unit_path()?;
    if path.exists() {
        std::fs::remove_file(&path)?;
        systemctl(&["daemon-reload"])?;
        println!("Service uninstalled.");
    } else {
        println!("Service was not installed.");
    }
    Ok(())
}
