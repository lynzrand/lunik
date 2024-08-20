mod channel;
mod init;

use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::Parser;
use home::home_dir;

use crate::{config::lunik_dir, mux::LUNIK_TOOLCHAIN_ENV_NAME};

/// The MoonBit toolchain multiplexer.
///
/// Symlink this binary with other names to call the corresponding tools.
#[derive(clap::Parser, Debug)]
struct Cli {
    #[clap(subcommand)]
    cmd: Cmd,
}

#[derive(clap::Parser, Debug)]
enum Cmd {
    Link(LinkSubcommand),

    InitConfig,

    Init(init::InitSubcommand),

    #[clap(subcommand)]
    Channel(channel::ChannelCommandline),

    Default(channel::DefaultSubcommand),

    Which(WhichSubcommand),
}

/// Symlink the current binary to the specified path(s).
#[derive(clap::Parser, Debug)]
#[clap(override_usage = "lunik link <PATH> \n    lunik link <PATH> <BINARYIES>...")]
struct LinkSubcommand {
    /// The target symlink path
    path: PathBuf,

    /// The binaries to symlink. If specified, `path` must be a directory.
    binaries: Vec<String>,

    /// Delete the target files if they exist.
    #[clap(short, long)]
    force: bool,
}

pub fn entry() -> anyhow::Result<()> {
    tracing_subscriber::FmtSubscriber::builder()
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(tracing_subscriber::filter::LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .init();

    let cli = Cli::parse();
    match &cli.cmd {
        Cmd::Link(link) => handle_link(&cli, link),
        Cmd::InitConfig => handle_init_config(),
        Cmd::Init(init) => init::handle_init(init),
        Cmd::Channel(cmd) => channel::entry(&cli, cmd),
        Cmd::Default(default) => channel::handle_default(&cli, default),
        Cmd::Which(which) => handle_which(&cli, which),
    }
}

fn handle_link(_cli: &Cli, cmd: &LinkSubcommand) -> anyhow::Result<()> {
    let self_path = std::env::current_exe().unwrap();

    let symlink_targets = if cmd.binaries.is_empty() {
        vec![cmd.path.clone()]
    } else {
        cmd.binaries
            .iter()
            .map(|binary| cmd.path.join(binary))
            .collect()
    };

    let mut any_failed = false;
    for target in symlink_targets {
        if target.exists() {
            if cmd.force {
                match std::fs::remove_file(&target) {
                    Ok(()) => {
                        println!("Removed existing file at {}", target.display());
                    }
                    Err(e) => {
                        eprintln!(
                            "Failed to remove existing file at {}: {}",
                            target.display(),
                            e
                        );
                        any_failed = true;
                        continue;
                    }
                }
            } else {
                eprintln!("Target file already exists at {}", target.display());
                any_failed = true;
                continue;
            }
        }

        match symlink_to(&self_path, &target) {
            Ok(()) => {
                println!("Symlinked {} to {}", self_path.display(), target.display());
            }
            Err(e) => {
                eprintln!(
                    "Failed to symlink {} to {}: {}",
                    self_path.display(),
                    target.display(),
                    e
                );
                any_failed = true;
            }
        }
    }

    if any_failed {
        anyhow::bail!("Some symlinks failed");
    } else {
        Ok(())
    }
}

pub fn symlink_to(from: &Path, to: &Path) -> anyhow::Result<()> {
    #[cfg(windows)]
    fn do_symlink(from: &Path, to: &Path) -> anyhow::Result<()> {
        std::os::windows::fs::symlink_file(from, to)?;
        Ok(())
    }

    #[cfg(unix)]
    fn do_symlink(from: &Path, to: &Path) -> anyhow::Result<()> {
        std::os::unix::fs::symlink(from, to)?;
        Ok(())
    }

    #[cfg(not(any(windows, unix)))]
    fn do_symlink(from: &Path, to: &Path) -> anyhow::Result<()> {
        panic!("Unsupported platform, unable to perform symlink");
    }

    do_symlink(from, to)
}

pub fn symlink_self_to(path: &Path) -> anyhow::Result<()> {
    let self_exe = std::env::current_exe()?;
    symlink_to(&self_exe, path)
}

fn handle_init_config() -> anyhow::Result<()> {
    let config_path = crate::config::config_path();
    if config_path.exists() {
        anyhow::bail!("Config file already exists at {}", config_path.display());
    }

    let default_config = crate::config::Config::default();
    let default_config_json = serde_json_lenient::to_string_pretty(&default_config)?;
    std::fs::write(&config_path, default_config_json)?;
    println!("Config file created at {}", config_path.display());

    Ok(())
}

/// Get the path of the binary in the specified toolchain.
#[derive(clap::Parser, Debug)]
#[clap(override_usage = "lunik which <BINARY> | lunik which <TOOLCHAIN> <BINARY>")]
struct WhichSubcommand {
    #[clap(hide(true))]
    arg1: String,

    #[clap(hide(true))]
    arg2: Option<String>,
}

fn handle_which(_cli: &Cli, cmd: &WhichSubcommand) -> anyhow::Result<()> {
    let cfg = crate::config::read_config()?;

    let binary = cmd.arg2.clone().unwrap_or(cmd.arg1.clone());
    let toolchain = if cmd.arg2.is_some() {
        Some(cmd.arg1.clone())
    } else {
        std::env::var(LUNIK_TOOLCHAIN_ENV_NAME).ok()
    };

    let executable_path = crate::mux::try_get_executable(&cfg, toolchain.as_deref(), &binary)?;
    println!("{}", executable_path.display());

    Ok(())
}
