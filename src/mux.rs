use std::path::PathBuf;

use crate::config::{Config, ToolchainInfo};

const LUNIK_TOOLCHAIN_ENV_NAME: &str = "LUNIK_TOOLCHAIN";

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

    let executable_path = try_get_executable(&cfg, mux_toolchain.as_deref(), binary_name)?;

    let mut cmd = std::process::Command::new(executable_path);
    cmd.args(argv);
    if let Some(toolchain) = mux_toolchain {
        cmd.env(LUNIK_TOOLCHAIN_ENV_NAME, toolchain);
    }
    let status = cmd.status()?;
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    Ok(())
}

fn try_get_executable(
    cfg: &Config,
    toolchain: Option<&str>,
    executable_name: &str,
) -> anyhow::Result<PathBuf> {
    let mut toolchain_name = toolchain.unwrap_or(&cfg.default);

    loop {
        // Get information about the toolchain
        let toolchain_info = cfg
            .toolchain
            .get(toolchain_name)
            .ok_or_else(|| anyhow::anyhow!("Toolchain not found: {}", toolchain_name))?;

        // Get the executable path
        let executable_path =
            get_toolchain_executable(toolchain_name, toolchain_info, executable_name);

        if executable_path.exists() {
            return Ok(executable_path);
        } else {
            // If the executable does not exist, try to use the fallback toolchain
            if let Some(fallback) = &toolchain_info.fallback {
                eprintln!(
                    "Executable not found in toolchain '{}', trying fallback '{}'",
                    toolchain_name, fallback
                );
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
        .unwrap_or_else(|| crate::config::toolchain_path(toolchain_name));
    let executable_name = if cfg!(windows) {
        format!("{}.exe", executable_name)
    } else {
        executable_name.to_string()
    };
    toolchain_root.join(executable_name)
}
