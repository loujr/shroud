use std::fs;
use std::path::PathBuf;

const DESKTOP_ENTRY: &str = r#"[Desktop Entry]
Type=Application
Name=Shroud VPN Manager
Comment=VPN connection manager with kill switch
Exec=shroud
Icon=network-vpn
Terminal=false
Categories=Network;System;
StartupNotify=false
X-GNOME-Autostart-enabled=true
"#;

pub struct Autostart;

impl Autostart {
    /// Get path to autostart desktop file
    fn desktop_file_path() -> Option<PathBuf> {
        dirs::config_dir().map(|c| c.join("autostart/shroud.desktop"))
    }

    /// Check if autostart is enabled
    pub fn is_enabled() -> bool {
        Self::desktop_file_path()
            .map(|p| p.exists())
            .unwrap_or(false)
    }

    /// Enable autostart
    pub fn enable() -> Result<(), String> {
        let path = Self::desktop_file_path().ok_or("Could not determine config directory")?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create autostart directory: {}", e))?;
        }

        fs::write(&path, DESKTOP_ENTRY)
            .map_err(|e| format!("Failed to write desktop file: {}", e))?;

        Ok(())
    }

    /// Disable autostart
    pub fn disable() -> Result<(), String> {
        let path = Self::desktop_file_path().ok_or("Could not determine config directory")?;

        if path.exists() {
            fs::remove_file(&path).map_err(|e| format!("Failed to remove desktop file: {}", e))?;
        }

        Ok(())
    }

    /// Toggle autostart
    pub fn toggle() -> Result<bool, String> {
        if Self::is_enabled() {
            Self::disable()?;
            Ok(false)
        } else {
            Self::enable()?;
            Ok(true)
        }
    }
}
