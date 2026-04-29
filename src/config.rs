use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct Config {
    pub root_dir: PathBuf,
    pub port: u16,
    pub timeout: u64,
    pub token: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Settings {
    token: String,
    created_at: String,
}

pub fn load_or_create_token() -> Result<String> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("cannot determine home directory"))?;
    let config_dir = home.join(".openlink");
    let settings_path = config_dir.join("settings.json");

    if settings_path.exists() {
        let data = std::fs::read_to_string(&settings_path)?;
        let settings: Settings = serde_json::from_str(&data)?;
        if !settings.token.is_empty() {
            return Ok(settings.token);
        }
    }

    // Generate new token: 32 random bytes -> 64 hex chars
    let mut bytes = [0u8; 32];
    rand::Rng::fill(&mut rand::rng(), &mut bytes);
    let token = hex::encode(bytes);

    let now = chrono::Utc::now().to_rfc3339();
    let settings = Settings {
        token: token.clone(),
        created_at: now,
    };

    std::fs::create_dir_all(&config_dir)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&config_dir, std::fs::Permissions::from_mode(0o700))?;
    }

    let json = serde_json::to_string_pretty(&settings)?;
    std::fs::write(&settings_path, json)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&settings_path, std::fs::Permissions::from_mode(0o600))?;
    }

    Ok(token)
}
