use std::{collections::HashMap, path::PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Config {
    /// Toolchain information
    #[serde(default)]
    pub toolchain: HashMap<String, ToolchainInfo>,

    /// Channel information
    #[serde(default)]
    pub channels: HashMap<String, ChannelInfo>,

    /// Default toolchain
    pub default: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
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

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ChannelInfo {
    /// Override URL
    url: Option<String>,
}

pub const MOON_HOME_DEFAULT: &str = ".moon";
pub const LUNIK_DIR: &str = "lunik";
pub const TOOLCHAIN_DEFAULT_ROOT: &str = "toolchain";
pub const CONFIG_NAME: &str = "lunik.json";
pub const LUNIK_HOME_ENV_NAME: &str = "LUNIK_HOME";
pub const MOON_HOME_ENV_NAME: &str = "MOON_HOME";

pub const BIN_DIR: &str = "bin";
pub const LIB_DIR: &str = "lib";

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

pub fn toolchain_path(toolchain_name: &str) -> PathBuf {
    lunik_dir()
        .join(TOOLCHAIN_DEFAULT_ROOT)
        .join(toolchain_name)
}

pub fn toolchain_bin_path(toolchain_name: &str) -> PathBuf {
    toolchain_path(toolchain_name).join(BIN_DIR)
}

pub fn toolchain_core_path(toolchain_name: &str) -> PathBuf {
    toolchain_path(toolchain_name).join(LIB_DIR)
}

pub fn read_config() -> anyhow::Result<Config> {
    let config_path = config_path();
    let cfg: Config = serde_json_lenient::from_reader(std::fs::File::open(config_path)?)?;
    Ok(cfg)
}

pub fn save_config(cfg: &Config) -> anyhow::Result<()> {
    let config_path = config_path();
    let mut file = std::fs::File::create(config_path)?;
    serde_json_lenient::to_writer_pretty(&mut file, cfg)?;
    Ok(())
}
