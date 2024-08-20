use std::path::{Path, PathBuf};

use clap::Parser;

#[derive(clap::Parser, Debug)]
struct Cli {
    #[clap(subcommand)]
    cmd: Cmd,
}

#[derive(clap::Parser, Debug)]
enum Cmd {
    Link(LinkSubcommand),
}

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
    fn do_symlink(from: &Path, to: &Path) {
        std::os::windows::fs::symlink_file(from, to).unwrap();
    }

    #[cfg(unix)]
    fn do_symlink(from: &Path, to: &Path) {
        std::os::unix::fs::symlink(from, to).unwrap();
    }

    #[cfg(not(any(windows, unix)))]
    fn do_symlink(from: &Path, to: &Path) {
        panic!("Unsupported platform, unable to perform symlink");
    }

    do_symlink(&self_path, &cmd.path);
    eprintln!(
        "Symlinked {} to {}",
        self_path.display(),
        cmd.path.display()
    );
}
