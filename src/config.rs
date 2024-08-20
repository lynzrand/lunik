use std::{collections::HashMap, path::PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Config {
    /// Toolchain information
    #[serde(default)]
    pub toolchain: HashMap<String, ToolchainInfo>,

    /// Default toolchain
    pub default: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ToolchainInfo {
    /// The fallback toolchain to use if the specified toolchain does not contain
    /// the required tool.
    pub fallback: Option<String>,

    /// The root path of the toolchain
    pub root_path: Option<PathBuf>,

    /// Tool path override
    #[serde(
        default,
        skip_serializing_if = "HashMap::is_empty",
        rename = "override"
    )]
    pub override_: HashMap<String, PathBuf>,
}

const MOON_HOME_DEFAULT: &str = ".moon";
const LUNIK_DIR: &str = "lunik";
const CONFIG_NAME: &str = "lunik.json";
const LUNIK_HOME_ENV_NAME: &str = "LUNIK_HOME";
const MOON_HOME_ENV_NAME: &str = "MOON_HOME";

/// Find the home directory.
///
/// 1. Try to find home directory from environment variables `LUNIK_HOME` and `MOON_HOME`.
/// 2. If not found, use the default home directory `~/.moon`.
pub fn home_dir() -> PathBuf {
    if let Ok(lunik_home) = std::env::var(LUNIK_HOME_ENV_NAME) {
        PathBuf::from(lunik_home)
    } else if let Ok(moon_home) = std::env::var(MOON_HOME_ENV_NAME) {
        PathBuf::from(moon_home)
    } else {
        home::home_dir().unwrap_or_default().join(MOON_HOME_DEFAULT)
    }
}

/// Find the resource dir for Lunik
pub fn lunik_dir() -> PathBuf {
    home_dir().join(LUNIK_DIR)
}

/// Find the config path. It is located at `{HOME_DIR}/lunik.json`
pub fn config_path() -> PathBuf {
    home_dir().join(CONFIG_NAME)
}

pub fn read_config() -> anyhow::Result<Config> {
    let config_path = config_path();
    let cfg: Config = serde_json_lenient::from_reader(std::fs::File::open(config_path)?)?;
    Ok(cfg)
}
