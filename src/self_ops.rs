mod channel;

use std::path::{Path, PathBuf};

use clap::Parser;

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

    #[clap(subcommand)]
    Channel(channel::ChannelCommandline),

    Default(channel::DefaultSubcommand),
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
        Cmd::InitConfig => handle_init_config(&cli),
        Cmd::Channel(cmd) => channel::entry(&cli, cmd),
        Cmd::Default(default) => channel::handle_default(&cli, default),
    }
}

fn handle_link(_cli: &Cli, cmd: &LinkSubcommand) -> anyhow::Result<()> {
    let self_path = std::env::current_exe().unwrap();

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

        match do_symlink(&self_path, &target) {
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

fn handle_init_config(_cli: &Cli) -> anyhow::Result<()> {
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
