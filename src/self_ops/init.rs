use std::{
    borrow::Cow,
    io::Write,
    path::{self, Path, PathBuf},
    process::Command,
};

use anyhow::Context;

use crate::config::{home_dir, lunik_dir, moon_bin_dir};

/// Performs all initialization and installation steps of lunik.
#[derive(clap::Parser, Debug)]
pub struct InitSubcommand {
    /// Automatically add the bin directory to PATH.
    #[clap(long)]
    auto: bool,

    /// Do not automatically add the bin directory to PATH. If neither `auto` nor `no-auto` is
    /// specified, you will be prompted to choose.
    #[clap(long)]
    no_auto: bool,

    /// Add to the specified shell. If not specified, the shell will be detected.
    #[clap(long)]
    shell: Option<String>,
}

pub fn handle_init(cmd: &InitSubcommand) -> anyhow::Result<()> {
    // First, create home dir, lunik dir and bin dir
    std::fs::create_dir_all(home_dir()).context("Failed to create home dir")?;
    std::fs::create_dir_all(moon_bin_dir()).context("Failed to create moon binary dir")?;
    std::fs::create_dir_all(lunik_dir()).context("Failed to create lunik home dir")?;

    // Copy the current executable to the bin dir
    let self_path = std::env::current_exe()?;
    let self_name = self_path.file_name().unwrap();
    let self_bin_path = moon_bin_dir().join(self_name);
    std::fs::copy(&self_path, &self_bin_path).context("Failed to copy self to bin dir")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = self_bin_path.metadata()?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(self_bin_path, perms)?;
    }

    // Init config
    super::handle_init_config().context("Failed to init config")?;

    // Ask the user to add the bin dir to PATH
    let shell = std::env::var("SHELL");
    let shell = cmd.shell.clone().or(shell.ok()).and_then(|s| to_shell(&s));
    let path = moon_bin_dir();

    let auto =
        shell.is_some() && (cmd.auto || (!cmd.no_auto && prompt_user_if_they_want_to_auto_edit()?));
    let mut auto_failed = false;
    if auto {
        let shell = shell.expect("Should not be None if auto is true");
        auto_failed = edit_shell_rc(shell, &path).is_err();
    }
    if auto_failed || !auto {
        prompt_user_to_manually_edit(shell, &path, auto_failed);
    }

    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum Shell {
    Bash,
    Zsh,
    Fish,
}

fn to_shell(s: &str) -> Option<Shell> {
    match s {
        "bash" => Some(Shell::Bash),
        "zsh" => Some(Shell::Zsh),
        "fish" => Some(Shell::Fish),
        _ => None,
    }
}

fn shell_rc_path(shell: Shell) -> PathBuf {
    let home = home_dir();
    match shell {
        Shell::Bash => home.join(".bashrc"),
        Shell::Zsh => home.join(".zshrc"),
        Shell::Fish => home.join(".config/fish/config.fish"),
    }
}

fn shell_rc_content(shell: Shell, path: &Path) -> String {
    match shell {
        Shell::Bash | Shell::Zsh => format!("export PATH=\"{}:$PATH\"\n", path.display()),
        Shell::Fish => format!("set -gx PATH {} $PATH\n", path.display()),
    }
}

fn edit_shell_rc(shell: Shell, path: &Path) -> anyhow::Result<()> {
    let rc_path = shell_rc_path(shell);
    let rc_content = shell_rc_content(shell, path);

    let rc_content = Cow::Borrowed(&rc_content);
    let rc_content = rc_content.as_bytes();

    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&rc_path)
        .context("Failed to open shell rc file")?;

    // Append `# moonbit lunik` to the shell rc file
    file.write_all(b"# moonbit lunik\n")
        .context("Failed to write to shell rc file")?;

    file.write_all(rc_content)
        .context("Failed to write to shell rc file")?;

    Ok(())
}

fn prompt_user_if_they_want_to_auto_edit() -> anyhow::Result<bool> {
    inquire::Confirm::new("Add MoonBit binaries to PATH?")
        .with_default(true)
        .prompt()
        .map_err(|e| e.into())
}

fn prompt_user_to_manually_edit(shell: Option<Shell>, path: &Path, auto_edit_failed: bool) {
    match shell {
        Some(shell) => {
            if auto_edit_failed {
                println!("We have failed to automatically edit your shell rc file.\n");
            }
            println!(
                "Please manually add the following line to {}:\n\n    {}",
                shell_rc_path(shell).display(),
                shell_rc_content(shell, path)
            );
        }
        None => {
            println!(
                "\
                We are unable to detect your shell.\n\
                Please manually add the following path to your PATH environment variable:\n\n    \
                {}",
                path.display()
            )
        }
    }
}
