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
