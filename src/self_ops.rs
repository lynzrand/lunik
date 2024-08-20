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
}

/// Symlink the current binary to the specified path.
#[derive(clap::Parser, Debug)]
struct LinkSubcommand {
    /// The target symlink path
    path: PathBuf,
}

pub fn entry() {
    let cli = Cli::parse();
    match &cli.cmd {
        Cmd::Link(link) => {
            handle_link(&cli, link);
        }
        Cmd::InitConfig => {
            handle_init_config(&cli);
        }
    }
}

fn handle_link(_cli: &Cli, cmd: &LinkSubcommand) {
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

    match do_symlink(&self_path, &cmd.path) {
        Ok(()) => {
            println!(
                "Symlinked {} to {}",
                self_path.display(),
                cmd.path.display()
            );
        }
        Err(e) => {
            eprintln!(
                "Failed to symlink {} to {}: {}",
                self_path.display(),
                cmd.path.display(),
                e
            );
        }
    }
}

fn handle_init_config(_cli: &Cli) {
    let config_path = crate::config::config_path();
    if config_path.exists() {
        eprintln!("Config file already exists at {}", config_path.display());
        return;
    }

    let default_config = crate::config::Config::default();
    let default_config_json = serde_json_lenient::to_string_pretty(&default_config).unwrap();
    std::fs::write(&config_path, default_config_json).unwrap();
    println!("Config file created at {}", config_path.display());
}
