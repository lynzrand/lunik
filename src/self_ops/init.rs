use std::borrow::Cow;

use anyhow::Context;

use crate::config::{home_dir, lunik_dir, moon_bin_dir};

/// Performs all initialization and installation steps of lunik.
#[derive(clap::Parser, Debug)]
pub struct InitSubcommand {}

pub fn handle_init() -> anyhow::Result<()> {
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
    match shell.as_deref() {
        Ok("bash") => {
            println!("Add the following line to your .bashrc or .bash_profile:");
            println!();
            println!("    export PATH=\"{}:$PATH\"", moon_bin_dir().display());
        }
        Ok("zsh") => {
            println!("Add the following line to your .zshrc:");
            println!();
            println!("    export PATH=\"{}:$PATH\"", moon_bin_dir().display());
        }
        Ok("fish") => {
            println!("Run the following command to add the bin dir to your PATH:");
            println!();
            println!("    fish_add_path {}", moon_bin_dir().display());
        }
        Ok(_) | Err(_) => {
            println!(
                "\
                We are unable to detect your shell.\n\
                Please manually add the following path to your PATH environment variable:\n\n    \
                {}",
                moon_bin_dir().display()
            )
        }
    }

    Ok(())
}
