use std::{borrow::Cow, path::PathBuf};

use crate::config::{Config, ToolchainInfo, LUNIK_HOME_ENV_NAME, MOON_HOME_ENV_NAME};
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

pub fn real_toolchain_name<'a>(
    cfg: &Config,
    toolchain_name: &'a str,
) -> anyhow::Result<Cow<'a, str>> {
    if cfg.toolchain.contains_key(toolchain_name) {
        Ok(Cow::Borrowed(toolchain_name))
    } else {
        let ch = toolchain_name.parse::<super::channel::Channel>()?;
        Ok(Cow::Owned(ch.to_string()))
    }
}

pub fn executable_entry(
    cfg: &Config,
    toolchain_name: Option<&str>,
    executable_name: &str,
) -> anyhow::Result<std::process::Command> {
    let executable_path = try_get_executable(cfg, toolchain_name, executable_name)?;
    let mut cmd = std::process::Command::new(executable_path);

    configure_cmd_environment(&mut cmd, toolchain_name, cfg)?;

    Ok(cmd)
}

pub fn configure_cmd_environment(
    cmd: &mut std::process::Command,
    toolchain_name: Option<&str>,
    cfg: &Config,
) -> Result<(), anyhow::Error> {
    cmd.env(LUNIK_HOME_ENV_NAME, crate::config::home_dir());
    if let Some(toolchain) = toolchain_name {
        cmd.env(LUNIK_TOOLCHAIN_ENV_NAME, toolchain);
    }
    cmd.env(
        MOON_HOME_ENV_NAME,
        try_get_toolchain_home(cfg, toolchain_name)?,
    );
    if std::env::var(crate::config::MOON_CORE_OVERRIDE_ENV_NAME).is_err() {
        let core_lib_path = try_get_core_lib(cfg, toolchain_name)?;
        cmd.env(crate::config::MOON_CORE_OVERRIDE_ENV_NAME, core_lib_path);
    };
    Ok(())
}

pub fn try_get_toolchain_home(
    cfg: &Config,
    toolchain_name: Option<&str>,
) -> anyhow::Result<PathBuf> {
    let initial_toolchain_name = toolchain_name.unwrap_or(&cfg.default);

    for (name, info) in cfg.toolchain_fallback_iter(initial_toolchain_name) {
        if info.fallback.is_none() {
            return Ok(info
                .root_path
                .clone()
                .unwrap_or_else(|| crate::config::toolchain_path(&name)));
        }
    }

    Err(anyhow::anyhow!(
        "No toolchain among the fallbacks of `{}` has a home path",
        initial_toolchain_name
    ))
}

pub fn try_get_executable(
    cfg: &Config,
    toolchain: Option<&str>,
    executable_name: &str,
) -> anyhow::Result<PathBuf> {
    let initial_toolchain_name = toolchain.unwrap_or(&cfg.default);

    // Strip .exe suffix from executable_name if on Windows
    let executable_name_base = if cfg!(windows) {
        executable_name.trim_end_matches(".exe")
    } else {
        executable_name
    };

    for (name, info) in cfg.toolchain_fallback_iter(initial_toolchain_name) {
        let executable_path = get_toolchain_executable(&name, info, executable_name_base);
        if executable_path.exists() {
            return Ok(executable_path);
        } else {
            // Optional: Add logging if needed
            // eprintln!(
            //     "Executable '{}' not found in toolchain '{}', trying next in fallback chain",
            //     executable_name, &name
            // );
        }
    }

    Err(anyhow::anyhow!(
        "Executable '{}' not found in toolchain '{}' or any of its fallbacks",
        executable_name,
        initial_toolchain_name
    ))
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
    let initial_toolchain_name = toolchain.unwrap_or(&cfg.default);

    for (name, info) in cfg.toolchain_fallback_iter(initial_toolchain_name) {
        let core_lib_path = get_toolchain_core_lib(&name, info);
        if core_lib_path.exists() {
            return Ok(core_lib_path);
        } else {
            eprintln!(
                "Core library not found in toolchain '{}', trying next in fallback chain",
                &name
            );
        }
    }

    Err(anyhow::anyhow!(
        "Core library not found in toolchain '{}' or any of its fallbacks",
        initial_toolchain_name
    ))
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
