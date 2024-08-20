use std::{borrow::Cow, path::PathBuf};

use crate::config::{Config, ToolchainInfo, LUNIK_HOME_ENV_NAME};

pub const LUNIK_TOOLCHAIN_ENV_NAME: &str = "LUNIK_TOOLCHAIN";

pub fn entry(binary_name: &str, argv: &[String]) -> anyhow::Result<()> {
    // Check if the next argument starts with "+"
    // If it does, it specifies which version of the toolchain to use
    // Otherwise, we check if we have specified the toolchain in the environment variable
    let mux_toolchain = argv
        .first()
        .and_then(|arg| arg.strip_prefix('+'))
        .map(|toolchain| toolchain.to_string());
    let toolchain_arg_present = mux_toolchain.is_some();
    let mux_toolchain = mux_toolchain.or_else(|| std::env::var(LUNIK_TOOLCHAIN_ENV_NAME).ok());

    let argv = if toolchain_arg_present {
        &argv[1..]
    } else {
        argv
    };

    let cfg = crate::config::read_config()?;

    let mut cmd = executable_entry(&cfg, mux_toolchain.as_deref(), binary_name)?;
    let cmd = cmd.args(argv);

    let status = cmd.status()?;
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    Ok(())
}

pub fn executable_entry(
    cfg: &Config,
    toolchain_name: Option<&str>,
    executable_name: &str,
) -> anyhow::Result<std::process::Command> {
    let executable_path = try_get_executable(cfg, toolchain_name, executable_name)?;
    let mut cmd = std::process::Command::new(executable_path);

    // Set env vars for children processes
    cmd.env(LUNIK_HOME_ENV_NAME, crate::config::home_dir());
    if let Some(toolchain) = toolchain_name {
        cmd.env(LUNIK_TOOLCHAIN_ENV_NAME, toolchain);
    }
    // Add core path override if not set
    if std::env::var(crate::config::MOON_CORE_OVERRIDE_ENV_NAME).is_err() {
        let core_lib_path = try_get_core_lib(cfg, toolchain_name)?;
        cmd.env(crate::config::MOON_CORE_OVERRIDE_ENV_NAME, core_lib_path);
    }

    Ok(cmd)
}

pub fn try_get_executable(
    cfg: &Config,
    toolchain: Option<&str>,
    executable_name: &str,
) -> anyhow::Result<PathBuf> {
    let mut toolchain_name = toolchain.unwrap_or(&cfg.default);

    // Strip .exe in executable if is Windows
    let executable_name = if cfg!(windows) {
        executable_name.trim_end_matches(".exe")
    } else {
        executable_name
    };

    loop {
        // Get information about the toolchain
        let (real_toolchain_name, toolchain_info) =
            if let Some(info) = cfg.toolchain.get(toolchain_name) {
                (Cow::Borrowed(toolchain_name), info)
            } else {
                match toolchain_name.parse::<super::channel::Channel>() {
                    Ok(ch) => {
                        let real_name = ch.to_string();
                        if let Some(info) = cfg.toolchain.get(&real_name) {
                            (Cow::Owned(real_name), info)
                        } else {
                            return Err(anyhow::anyhow!("Toolchain not found: {}", toolchain_name));
                        }
                    }
                    Err(_) => {
                        return Err(anyhow::anyhow!("Toolchain not found: {}", toolchain_name));
                    }
                }
            };

        // Get the executable path
        let executable_path =
            get_toolchain_executable(&real_toolchain_name, toolchain_info, executable_name);

        if executable_path.exists() {
            return Ok(executable_path);
        } else {
            // If the executable does not exist, try to use the fallback toolchain
            if let Some(fallback) = &toolchain_info.fallback {
                // eprintln!(
                //     "Executable not found in toolchain '{}', trying fallback '{}'",
                //     real_toolchain_name, fallback
                // );
                toolchain_name = fallback;
            } else {
                return Err(anyhow::anyhow!(
                    "Executable not found: {}",
                    executable_path.display()
                ));
            }
        }
    }
}

fn get_toolchain_executable(
    toolchain_name: &str,
    toolchain: &ToolchainInfo,
    executable_name: &str,
) -> PathBuf {
    if let Some(path) = toolchain.override_.get(executable_name) {
        return path.clone();
    }

    let toolchain_root = toolchain
        .root_path
        .clone()
        .unwrap_or_else(|| crate::config::toolchain_path(toolchain_name))
        .join("bin");
    let executable_name = if cfg!(windows) {
        format!("{}.exe", executable_name)
    } else {
        executable_name.to_string()
    };
    toolchain_root.join(executable_name)
}

fn try_get_core_lib(cfg: &Config, toolchain: Option<&str>) -> anyhow::Result<PathBuf> {
    let mut toolchain_name = toolchain.unwrap_or(&cfg.default);

    loop {
        // Get information about the toolchain
        let (real_toolchain_name, toolchain_info) =
            if let Some(info) = cfg.toolchain.get(toolchain_name) {
                (Cow::Borrowed(toolchain_name), info)
            } else {
                match toolchain_name.parse::<super::channel::Channel>() {
                    Ok(ch) => {
                        let real_name = ch.to_string();
                        if let Some(info) = cfg.toolchain.get(&real_name) {
                            (Cow::Owned(real_name), info)
                        } else {
                            return Err(anyhow::anyhow!("Toolchain not found: {}", toolchain_name));
                        }
                    }
                    Err(_) => {
                        return Err(anyhow::anyhow!("Toolchain not found: {}", toolchain_name));
                    }
                }
            };

        // Get the executable path
        let core_lib_path = get_toolchain_core_lib(&real_toolchain_name, toolchain_info);

        if core_lib_path.exists() {
            return Ok(core_lib_path);
        } else {
            // If the executable does not exist, try to use the fallback toolchain
            if let Some(fallback) = &toolchain_info.fallback {
                eprintln!(
                    "Core library not found in toolchain '{}', trying fallback '{}'",
                    real_toolchain_name, fallback
                );
                toolchain_name = fallback;
            } else {
                return Err(anyhow::anyhow!(
                    "Core library not found: {}",
                    core_lib_path.display()
                ));
            }
        }
    }
}

fn get_toolchain_core_lib(toolchain_name: &str, toolchain: &ToolchainInfo) -> PathBuf {
    if let Some(path) = &toolchain.core_path {
        return path.clone();
    }

    toolchain
        .root_path
        .clone()
        .unwrap_or_else(|| crate::config::toolchain_path(toolchain_name))
        .join("lib/core")
}
