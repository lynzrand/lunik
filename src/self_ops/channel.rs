//! Toolchain management.

use std::{cell::Cell, path::Path};

use anyhow::Context;
use indicatif::ProgressStyle;
use sha2::Digest;
use tempfile::TempDir;

use crate::{
    channel::{Channel, ChannelKind},
    config::{read_config, save_config, ChannelInfo, Config, ToolchainInfo, BIN_DIR, LIB_DIR},
    mux::real_toolchain_name,
};

use super::symlink_self_to;

const MOONBIT_CLI_WEB: &str = "https://cli.moonbitlang.com";

fn channel_cli_file_url(ch: &Channel) -> String {
    format!(
        "{base}/binaries/{ver}/moonbit-{tgt}.tar.gz",
        base = MOONBIT_CLI_WEB,
        ver = ch.channel,
        tgt = ch.host
    )
}

fn channel_core_file_url(ch: &Channel) -> String {
    if ch.channel == ChannelKind::Bleeding {
        // https://docs.github.com/en/repositories/working-with-files/using-files/downloading-source-code-archives#source-code-archive-urls
        return "https://github.com/moonbitlang/core/archive/refs/heads/main.tar.gz".into();
    }
    format!(
        "{base}/cores/core-{ver}.tar.gz",
        base = MOONBIT_CLI_WEB,
        ver = ch.channel,
    )
}

fn channel_sha_url(ch: &Channel) -> String {
    format!(
        "{base}/binaries/{ver}/moonbit-{tgt}.sha256",
        base = MOONBIT_CLI_WEB,
        ver = ch.channel,
        tgt = ch.host
    )
}

const PROGRESS_BAR_TEMPLATE: &str =
    "{prefix} [{elapsed_precise}] [{bar}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})";

fn download_file(
    client: &mut reqwest::blocking::Client,
    url: &str,
    target: &std::path::Path,
    display_name: &str,
    quiet: bool,
) -> anyhow::Result<()> {
    let response = client.get(url).send()?;
    let mut response = response.error_for_status()?;

    let bar = match response.content_length() {
        _ if quiet => indicatif::ProgressBar::hidden(),
        Some(len) => indicatif::ProgressBar::new(len),
        None => indicatif::ProgressBar::new_spinner(),
    };
    let bar = bar.with_prefix(display_name.to_owned()).with_style(
        ProgressStyle::with_template(PROGRESS_BAR_TEMPLATE)
            .unwrap()
            .progress_chars("#> "),
    );

    let mut reader = bar.wrap_read(&mut response);

    let output_file = std::fs::File::create(target)?;
    let mut writer = std::io::BufWriter::new(output_file);
    std::io::copy(&mut reader, &mut writer)?;

    Ok(())
}

fn untar(tarball: &std::path::Path, target: &std::path::Path) -> anyhow::Result<()> {
    if tracing::span_enabled!(tracing::Level::DEBUG) {
        tracing::debug!("Untarring {} to {}", tarball.display(), target.display());
        // Print the contents of the tarball
        let tar_gz = std::fs::File::open(tarball)?;
        let tar = flate2::read::GzDecoder::new(tar_gz);
        let mut archive = tar::Archive::new(tar);
        for it in archive
            .entries()
            .context("Failed to open the TAR archive")?
        {
            let entry = it?;
            let path = entry.path()?;
            tracing::debug!("Entry: {}", path.display());
        }
    }

    let tar_gz = std::fs::File::open(tarball)?;
    let tar = flate2::read::GzDecoder::new(tar_gz);
    let mut archive = tar::Archive::new(tar);
    archive.unpack(target)?;

    Ok(())
}

fn verify_outputs(target_dir: &std::path::Path, sha_info: &str) -> anyhow::Result<()> {
    let info = sha_info
        .lines()
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .map(|x| x.split_once("  ").unwrap());

    for (shasum, filename) in info {
        let filename = target_dir.join(filename);
        let file = std::fs::File::open(&filename)
            .with_context(|| format!("Failed to open file: {}", filename.display()))?;

        let mut hasher = sha2::Sha256::new();
        let mut reader = std::io::BufReader::new(file);
        std::io::copy(&mut reader, &mut hasher)
            .with_context(|| format!("Failed to read file: {}", filename.display()))?;

        let actual = hasher.finalize();
        let actual = hex::encode(actual);

        if actual != shasum {
            anyhow::bail!(
                "Checksum mismatch for file: {}. Expected: {}, actual: {}",
                filename.display(),
                shasum,
                actual
            );
        }
    }

    Ok(())
}

#[cfg(unix)]
fn add_executable_permissions(path: &std::path::Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut perms = path.metadata()?.permissions();
    let mode = perms.mode();
    perms.set_mode(mode | 0o111);
    std::fs::set_permissions(path, perms)?;

    Ok(())
}

#[cfg(unix)]
fn add_permissions_recursive(path: &std::path::Path) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();
        let ty = entry.file_type()?;
        if ty.is_dir() {
            add_permissions_recursive(&path)?;
        } else if ty.is_file() {
            add_executable_permissions(&path).with_context(|| {
                format!("Failed to add executable permissions to {}", path.display())
            })?;
        }
    }

    Ok(())
}

fn run_bundle_core(
    config: &Config,
    core_dir: &std::path::Path,
    channel: &Channel,
) -> anyhow::Result<()> {
    // moon bundle --all --source-dir <core_dir>
    let mut cmd = crate::mux::executable_entry(config, Some(&channel.to_string()), "moon")
        .context("Failed to find executable `moon`")?;
    cmd.args(["bundle", "--all", "--source-dir"]);
    cmd.arg(core_dir);

    tracing::debug!("Running command: {:?}", cmd);

    let mut child = cmd.spawn().context("Failed to spawn `moon`")?;
    let status = child.wait().context("Failed to run `moon`")?;
    if !status.success() {
        anyhow::bail!(
            "Failed to bundle core: `moon bundle --all` failed with status {}",
            status
        );
    }

    Ok(())
}

/// List of suffixes that are definitely not executables
const EXECUTABLE_SUFFIX_BLACKLIST: &[&str] = &[".so", ".dylib", ".dll"];

/// Get if the path might be an executable. The file currently does not have
/// the executable bit set on Unix-like systems.
fn can_be_executable(path: &std::path::Path) -> bool {
    if !path.is_file() {
        return false;
    }

    if EXECUTABLE_SUFFIX_BLACKLIST
        .iter()
        .any(|suffix| path.ends_with(suffix))
    {
        return false;
    }

    #[cfg(unix)]
    {
        true
    }

    #[cfg(windows)]
    {
        path.extension().map_or(false, |ext| ext == "exe")
    }

    #[cfg(not(any(unix, windows)))]
    {
        compile_error!("Currently does not support OSes other than Unix-like and Windows");
    }
}

fn ensure_all_executables_are_linked(bin_dir: &std::path::Path) -> anyhow::Result<()> {
    let moon_bin_dir = crate::config::moon_bin_dir();
    // ensure bin dir exists
    std::fs::create_dir_all(&moon_bin_dir).context(format!(
        "Failed to create the bin directory {}",
        moon_bin_dir.display()
    ))?;

    // Enumerate executables in the bin directory
    let entries = std::fs::read_dir(bin_dir)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if can_be_executable(&path) {
            let filename = path.file_name().unwrap().to_string_lossy();
            let exe_path = moon_bin_dir.join(&*filename);
            if !exe_path.exists() {
                symlink_self_to(&exe_path)
                    .with_context(|| format!("Failed to create symlink {}", exe_path.display()))?;
                // if unix, add executable permissions
                #[cfg(unix)]
                {
                    add_executable_permissions(&exe_path).with_context(|| {
                        format!(
                            "Failed to add executable permissions to {}",
                            exe_path.display()
                        )
                    })?;
                }
                tracing::info!("Linked {}", exe_path.display());
            }
        }
    }

    Ok(())
}

/// Perform a full installation of the toolchain.
fn full_install(
    config: &Config,
    client: &mut reqwest::blocking::Client,
    channel: &Channel,
    // the directory to install into
    target_dir: &std::path::Path,
    // the parent directory of the target directory, for temp files
    target_parent_dir: &std::path::Path,
    quiet: bool,
) -> anyhow::Result<()> {
    let files_url = channel_cli_file_url(channel);
    let core_url = channel_core_file_url(channel);
    let sha_url = channel_sha_url(channel);

    std::fs::create_dir_all(target_dir).context("Failed to create the installation dir")?;

    tracing::info!("Begin installation in channel {}", channel);

    // Download and unpack in a temporary directory
    let tempdir_ =
        TempDir::with_prefix_in(format!("lunik-install-{}", channel), target_parent_dir)?;
    let tempdir = tempdir_.path();
    tracing::debug!("Using temporary directory: {}", tempdir.display());

    let files_tarball = tempdir.join("bin.tar.gz");
    let core_tarball = tempdir.join("core.tar.gz");

    tracing::info!("Downloading files");
    tracing::debug!(
        "Downloading MoonBit binaries and libraries from {}",
        files_url
    );
    download_file(
        client,
        &files_url,
        &files_tarball,
        "MoonBit binaries",
        quiet,
    )
    .context(
        "Failed to download MoonBit binaries. You might want to check if the version exists.",
    )?;
    tracing::debug!("Downloading MoonBit core from {}", core_url);
    download_file(client, &core_url, &core_tarball, "MoonBit core", quiet).context(
        "Failed to download MoonBit core. You might want to check if the version exists.",
    )?;

    let temp_bin_dir = tempdir.join(BIN_DIR);
    let temp_lib_dir = tempdir.join(LIB_DIR);

    tracing::info!("Unpacking files");
    tracing::debug!("Unpacking MoonBit files to {}", tempdir.display());
    untar(&files_tarball, tempdir).context("Failed to unpack MoonBit files")?;
    tracing::debug!("Unpacking MoonBit core to {}", temp_lib_dir.display());
    untar(&core_tarball, &temp_lib_dir).context("Failed to unpack MoonBit core")?;

    // Rename the first `core-*/` under `temp_lib_dir` to `core/` if there is one.
    // This is because the `core` tarball from GitHub, once extracted,
    // will become a directory named `core-<github.ref>`.
    let maybe_branched_core_dir = temp_lib_dir.read_dir()?.find_map(|entry| {
        let path = entry.ok()?.path();
        (path.is_dir() && path.file_name()?.to_string_lossy().starts_with("core-")).then_some(path)
    });
    if let Some(branched_core_dir) = maybe_branched_core_dir {
        let core_dir = temp_lib_dir.join("core");
        tracing::debug!(
            "Renaming MoonBit core directory from {} to {}",
            branched_core_dir.display(),
            core_dir.display()
        );
        std::fs::rename(branched_core_dir, &core_dir)?;
    }

    // Check the contents of the temp dir and bin dir
    if tracing::event_enabled!(tracing::Level::DEBUG) {
        tracing::debug!("Contents of temp dir:");
        for entry in std::fs::read_dir(tempdir)? {
            let entry = entry?;
            let path = entry.path();
            tracing::debug!("Entry: {}", path.display());
        }

        tracing::debug!("Contents of temp bin dir:");
        for entry in std::fs::read_dir(&temp_bin_dir)? {
            let entry = entry?;
            let path = entry.path();
            tracing::debug!("Entry: {}", path.display());
        }
    }

    #[cfg(unix)]
    {
        tracing::debug!(
            "Adding executable permissions recursively to {}",
            temp_bin_dir.display()
        );
        add_permissions_recursive(&temp_bin_dir)
            .context("Failed to add permissions recursively")?;
    }

    // tracing::info!("Verifying checksums");
    // tracing::debug!("Fetching checksum info from {}", sha_url);
    // let sha_info = client.get(sha_url).send()?.text()?;
    // tracing::debug!(
    //     "Verifying checksums for files in {}",
    //     temp_bin_dir.display()
    // );
    // verify_outputs(&temp_bin_dir, &sha_info).context("Failed to verify checksums")?;

    tracing::info!("Download completed");
    tracing::info!("Moving files to their installation location");

    let bin_dir = target_dir.join(BIN_DIR);
    let lib_dir = target_dir.join(LIB_DIR);

    // Move to the final location
    // Rename the old directory if it exists
    let update_successful = Cell::new(false);
    let backup_dir = target_parent_dir.join(format!("{}-backup", channel));
    // If anything fails, we will roll back the changes
    scopeguard::defer! {
        if !update_successful.get() {
            tracing::warn!("Installation failed, rolling back changes");

            // Delete the new directories

            // Move back the old directories
            std::fs::rename(&backup_dir, target_dir).ok();
        }
    }

    // Remove any existing backup directories
    if backup_dir.exists() {
        tracing::debug!("Removing old backup directory {}", backup_dir.display());
        std::fs::remove_dir_all(&backup_dir).context("Failed to remove old bin backup dir")?;
    }

    // Backup the current directories and install the new ones
    std::fs::rename(target_dir, &backup_dir).ok();

    // Move the new directories to the final location
    std::fs::rename(tempdir, target_dir).context("Failed to move new directories")?;

    // Ensure everything in /bin exist in home directory
    tracing::debug!(
        "Ensuring all executables are linked in {}",
        bin_dir.display()
    );
    ensure_all_executables_are_linked(&bin_dir)
        .context("Failed to symlink some executables to bin directory")?;

    // Check moon and moonrun versions
    let moon_version = crate::mux::executable_entry(config, Some(&channel.to_string()), "moon")
        .context("Failed to find executable `moon`")?
        .arg("version")
        .output()
        .context("Failed to run `moon version`")?;
    let moon_version = String::from_utf8_lossy(&moon_version.stdout);
    let moonrun_version =
        crate::mux::executable_entry(config, Some(&channel.to_string()), "moonrun")
            .context("Failed to find executable `moonrun`")?
            .arg("--version")
            .output()
            .context("Failed to run `moonrun --version`")?;
    let moonrun_version = String::from_utf8_lossy(&moonrun_version.stdout);
    tracing::info!("Installed moon version: {}", moon_version.trim());
    tracing::info!("Installed moonrun version: {}", moonrun_version.trim());

    // Compile core libraries
    tracing::info!("Compiling core libraries");
    run_bundle_core(config, &lib_dir.join("core"), channel)
        .context("Failed to compile core libraries")?;

    // Okay, we are done
    update_successful.set(true);

    if backup_dir.exists() {
        std::fs::remove_dir_all(&backup_dir).context("Failed to remove backup dir")?;
    }

    tracing::info!("Installation completed");

    Ok(())
}

#[derive(Debug, clap::Parser)]
pub enum ChannelCommandline {
    /// Add a toolchain channel
    Add(AddSubcommand),
    /// Update a toolchain channel or all channels
    Update(UpdateSubcommand),
    /// Remove a toolchain channels
    Remove(RemoveSubcommand),
    /// List installed toolchain channels
    List(ListSubcommand),
    /// Specify the default toolchain. Same as `lunik default`
    Default(DefaultSubcommand),
}

#[derive(Debug, clap::Parser)]
pub struct AddSubcommand {
    /// The toolchain to add
    channel: String,
}

fn handle_add(_cli: &super::Cli, cmd: &AddSubcommand) -> anyhow::Result<()> {
    let old_config = read_config().context("When reading config")?;
    let channel: Channel = cmd.channel.parse().context("parsing toolchain channel")?;
    let channel_name = channel.to_string();

    if old_config.channels.contains_key(&channel_name) {
        anyhow::bail!("Toolchain channel already exists: {}", cmd.channel);
    }

    // Update the config
    let mut new_config = old_config.clone();
    let channel_info = ChannelInfo::default();
    new_config
        .channels
        .insert(channel_name.clone(), channel_info);
    let toolchain_info = ToolchainInfo::default();
    new_config
        .toolchain
        .insert(channel_name.clone(), toolchain_info);

    // save the config so that other lunik instances can use it
    save_config(&new_config)?;

    // Do the installation
    let mut client = reqwest::blocking::Client::new();
    let toolchain_root = crate::config::toolchain_root();
    let path = crate::config::toolchain_path(&channel_name);
    match full_install(
        &new_config,
        &mut client,
        &channel,
        &path,
        &toolchain_root,
        false,
    ) {
        Ok(_) => {}
        Err(e) => {
            // If the installation fails, restore the old config
            save_config(&old_config)?;
            return Err(e);
        }
    };

    println!("Toolchain installed: {}", cmd.channel);

    Ok(())
}

#[derive(Debug, clap::Parser)]
pub struct UpdateSubcommand {
    /// The toolchain to update. If not specified, update all toolchains.
    channel: Vec<String>,
}

fn handle_update(_cli: &super::Cli, cmd: &UpdateSubcommand) -> anyhow::Result<()> {
    let config = read_config().context("When reading config")?;
    let channels = if cmd.channel.is_empty() {
        config.channels.keys().cloned().collect()
    } else {
        cmd.channel.clone()
    };

    let mut client = reqwest::blocking::Client::new();
    for channel in channels {
        let toolchain: Channel = channel.parse().context("parsing toolchain channel")?;
        full_install(
            &config,
            &mut client,
            &toolchain,
            &crate::config::toolchain_path(&channel),
            &crate::config::toolchain_root(),
            false,
        )?;
        println!("Toolchain updated: {}", channel);
    }

    Ok(())
}

#[derive(Debug, clap::Parser)]
pub struct RemoveSubcommand {
    /// The toolchain to remove
    channel: String,
}

fn handle_remove(_cli: &super::Cli, cmd: &RemoveSubcommand) -> anyhow::Result<()> {
    let config = read_config().context("When reading config")?;
    let channel: Channel = cmd
        .channel
        .parse()
        .context("When parsing toolchain channel")?;
    let channel_name = channel.to_string();

    if !config.channels.contains_key(&channel_name) {
        anyhow::bail!("Toolchain channel not found: {}", cmd.channel);
    }

    let channel_path = crate::config::toolchain_path(&cmd.channel);
    if channel_path.exists() {
        std::fs::remove_dir_all(&channel_path)?;
    }

    let mut config = config;
    config.channels.remove(&channel_name);
    config.toolchain.remove(&channel_name);
    save_config(&config)?;

    println!("Toolchain removed: {}", cmd.channel);

    Ok(())
}

#[derive(Debug, clap::Parser)]
pub struct ListSubcommand {}

fn handle_list(_cli: &super::Cli, _cmd: &ListSubcommand) -> anyhow::Result<()> {
    let config = read_config().context("When reading config")?;
    for name in config.toolchain.keys() {
        println!("{}", name);
    }

    Ok(())
}

/// Specify the default toolchain
#[derive(clap::Parser, Debug)]
pub struct DefaultSubcommand {
    /// The default toolchain name
    toolchain: String,
}

pub fn handle_default(_cli: &super::Cli, cmd: &DefaultSubcommand) -> anyhow::Result<()> {
    let mut config = crate::config::read_config()?;

    let toolchain_name = real_toolchain_name(&config, &cmd.toolchain)?;
    config.default = toolchain_name.clone().into();

    println!("Default toolchain set to {}", cmd.toolchain);

    symlink_dir_to_default(&toolchain_name, "lib")?;
    symlink_file_to_default(&toolchain_name, "bin/lsp-server.js")?;

    crate::config::save_config(&config).context("Unable to save configuration")?;
    Ok(())
}

/// Delete a file in $MOON_HOME and symlink it to the equivalent file in the specified toolchain
fn symlink_file_to_default(toolchain_name: &str, file_path: &str) -> anyhow::Result<()> {
    let target_file = crate::config::home_dir().join(file_path);
    let default_toolchain_path = crate::config::toolchain_path(toolchain_name);
    let toolchain_target_file = default_toolchain_path.join(file_path);

    // Ensure the parent directory of the target file exists
    if let Some(parent) = target_file.parent() {
        std::fs::create_dir_all(parent).context(format!(
            "Unable to create parent directory for {}",
            file_path
        ))?;
    }

    // Ensure the source file exists in the toolchain
    if !toolchain_target_file.exists() {
        anyhow::bail!(
            "Source file does not exist in toolchain: {}",
            toolchain_target_file.display()
        );
    }

    // Remove the original file or symlink if it exists
    if target_file.try_exists()? || target_file.is_symlink() {
        tracing::info!("Removing file: {}", target_file.display());
        std::fs::remove_file(&target_file)
            .context(format!("Unable to remove file {}", target_file.display()))?;
    }

    // Create the symlink
    match symlink_core(&toolchain_target_file, &target_file) {
        Ok(_) => {
            tracing::info!(
                "Symlinked file: {} -> {}",
                toolchain_target_file.display(),
                target_file.display()
            );
        }
        Err(e) => {
            tracing::error!(
                "Unable to symlink file: {}; Path: {}",
                e,
                toolchain_target_file.display()
            );
            return Err(e);
        }
    };

    Ok(())
}

/// Delete a directory in $MOON_HOME and symlink it to the equivalent directory in the specified toolchain
fn symlink_dir_to_default(toolchain_name: &str, dir_path: &str) -> anyhow::Result<()> {
    let target_dir = crate::config::home_dir().join(dir_path);
    let default_toolchain_path = crate::config::toolchain_path(toolchain_name);
    let toolchain_target_dir = default_toolchain_path.join(dir_path);

    // Ensure the target directory exists in the toolchain
    std::fs::create_dir_all(&toolchain_target_dir)
        .context(format!("Unable to create {} directory", dir_path))?;

    // Remove (or unlink) the original directory before symlinking.
    // Note that the original directory can be either a symlink (we are already handling toolchains)
    // or a directory (this is a new installation).
    if target_dir.try_exists()? || target_dir.is_symlink() {
        // Either the directory exists (or is a valid symlink), or it is an invalid symlink.
        tracing::info!("Removing directory: {}", target_dir.display());
        std::fs::remove_dir_all(&target_dir).context(format!(
            "Unable to remove directory {}",
            target_dir.display()
        ))?;
    }

    match symlink_core(&toolchain_target_dir, &target_dir) {
        Ok(_) => {
            tracing::info!(
                "Symlinked directory: {} -> {}",
                toolchain_target_dir.display(),
                target_dir.display()
            );
        }
        Err(e) => {
            tracing::error!(
                "Unable to symlink directory: {}; Path: {}",
                e,
                toolchain_target_dir.display()
            );
            return Err(e);
        }
    };

    Ok(())
}

fn symlink_core(default_core_dir: &Path, core_dir: &Path) -> Result<(), anyhow::Error> {
    #[cfg(unix)]
    std::os::unix::fs::symlink(default_core_dir, core_dir)?;
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(default_core_dir, core_dir)?;
    #[cfg(not(any(unix, windows)))]
    bail!("Unsupported platform");
    Ok(())
}

pub fn entry(cli: &super::Cli, cmd: &ChannelCommandline) -> anyhow::Result<()> {
    match cmd {
        ChannelCommandline::Default(v) => handle_default(cli, v),
        ChannelCommandline::Add(v) => handle_add(cli, v),
        ChannelCommandline::Update(v) => handle_update(cli, v),
        ChannelCommandline::Remove(v) => handle_remove(cli, v),
        ChannelCommandline::List(v) => handle_list(cli, v),
    }
}
