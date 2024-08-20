//! Toolchain management.

use anyhow::Context;
use sha2::Digest;
use tempfile::TempDir;

use crate::{
    channel::{Channel, ChannelKind},
    config::{read_config, save_config, ChannelInfo, BIN_DIR, CORE_DIR},
};

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
    let bar = bar.with_message(display_name.to_owned());
    let mut reader = bar.wrap_read(&mut response);

    let output_file = std::fs::File::create(target)?;
    let mut writer = std::io::BufWriter::new(output_file);
    std::io::copy(&mut reader, &mut writer)?;

    Ok(())
}

fn untar(tarball: &std::path::Path, target: &std::path::Path) -> anyhow::Result<()> {
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
        let file = std::fs::File::open(&filename)?;

        let mut hasher = sha2::Sha256::new();
        let mut reader = std::io::BufReader::new(file);
        std::io::copy(&mut reader, &mut hasher)?;

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

fn full_install(
    client: &mut reqwest::blocking::Client,
    channel: &Channel,
    target_dir: &std::path::Path,
    quiet: bool,
) -> anyhow::Result<()> {
    let bin_dir = target_dir.join(BIN_DIR);
    let core_dir = target_dir.join(CORE_DIR);

    let bin_url = channel_cli_file_url(channel);
    let core_url = channel_core_file_url(channel);
    let sha_url = channel_sha_url(channel);

    tracing::info!("Begin installation in channel {}", channel);

    // Download and unpack in a temporary directory
    let tempdir_ = TempDir::with_prefix(format!("lunik-install-{}", channel))?;
    let tempdir = tempdir_.path();
    tracing::debug!("Using temporary directory: {}", tempdir.display());

    let bin_tarball = tempdir.join("bin.tar.gz");
    let core_tarball = tempdir.join("core.tar.gz");

    tracing::info!("Downloading files");
    download_file(client, &bin_url, &bin_tarball, "moon binaries", quiet)
        .context("When downloading moon binaries")?;
    download_file(client, &core_url, &core_tarball, "moon core", quiet)
        .context("When downloading moon core")?;

    let temp_bin_dir = tempdir.join("bin");
    let temp_core_dir = tempdir.join("core");

    tracing::info!("Unpacking files");

    untar(&bin_tarball, &temp_bin_dir).context("When unpacking moon binaries")?;
    untar(&core_tarball, &temp_core_dir).context("When unpacking moon core")?;

    tracing::info!("Verifying checksums");

    // Verify checksums
    let sha_info = client.get(sha_url).send()?.text()?;
    verify_outputs(&temp_bin_dir, &sha_info)?;

    tracing::info!("Installing files");

    // Move to the final location
    std::fs::create_dir_all(target_dir)?;
    // Rename the old directory if it exists
    let backup_dir = target_dir.join(format!("{}-backup", BIN_DIR));
    if bin_dir.exists() {
        std::fs::rename(&bin_dir, &backup_dir)?;
        tracing::info!("Moved existing bin directory to {}", backup_dir.display());
    }
    std::fs::rename(&temp_bin_dir, &bin_dir)?;
    std::fs::remove_dir_all(&backup_dir).ok();

    let backup_dir = target_dir.join(format!("{}-backup", CORE_DIR));
    if core_dir.exists() {
        std::fs::rename(&core_dir, &backup_dir)?;
        tracing::info!("Moved existing core directory to {}", backup_dir.display());
    }
    std::fs::rename(&temp_core_dir, &core_dir)?;
    std::fs::remove_dir_all(&backup_dir).ok();

    tracing::info!("Installation completed");

    Ok(())
}

#[derive(Debug, clap::Parser)]
pub enum ToolchainCommandline {
    /// Install a toolchain
    Install(AddSubcommand),
    /// Update a toolchain or all toolchains
    Update(UpdateSubcommand),
    /// Uninstall a toolchain
    Uninstall(RemoveSubcommand),
    /// List installed toolchains
    List(ListSubcommand),
    /// Specify the default toolchain. Same as `lunik default`
    Default(super::DefaultSubcommand),
}

#[derive(Debug, clap::Parser)]
pub struct AddSubcommand {
    /// The toolchain to add
    channel: String,
}

fn handle_add(cli: &super::Cli, cmd: &AddSubcommand) -> anyhow::Result<()> {
    let config = read_config().context("When reading config")?;
    let toolchain: Channel = cmd.channel.parse().context("parsing toolchain channel")?;
    if config.channels.contains_key(&cmd.channel) {
        anyhow::bail!("Toolchain channel already exists: {}", cmd.channel);
    }

    // Do the installation
    let mut client = reqwest::blocking::Client::new();
    full_install(
        &mut client,
        &toolchain,
        &crate::config::toolchain_path(&cmd.channel),
        false,
    )?;

    // Update the config
    let mut config = config;
    let channel_info = ChannelInfo::default();
    config.channels.insert(cmd.channel.clone(), channel_info);
    save_config(&config)?;

    Ok(())
}

#[derive(Debug, clap::Parser)]
pub struct UpdateSubcommand {
    /// The toolchain to update. If not specified, update all toolchains.
    channel: Vec<String>,
}

fn handle_update(cli: &super::Cli, cmd: &UpdateSubcommand) -> anyhow::Result<()> {
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
            &mut client,
            &toolchain,
            &crate::config::toolchain_path(&channel),
            false,
        )?;
    }

    Ok(())
}

#[derive(Debug, clap::Parser)]
pub struct RemoveSubcommand {
    /// The toolchain to remove
    channel: String,
}

fn handle_remove(cli: &super::Cli, cmd: &RemoveSubcommand) -> anyhow::Result<()> {
    let config = read_config().context("When reading config")?;
    if !config.channels.contains_key(&cmd.channel) {
        anyhow::bail!("Toolchain channel not found: {}", cmd.channel);
    }

    let channel_path = crate::config::toolchain_path(&cmd.channel);
    if channel_path.exists() {
        std::fs::remove_dir_all(&channel_path)?;
    }

    let mut config = config;
    config.channels.remove(&cmd.channel);
    save_config(&config)?;

    Ok(())
}

#[derive(Debug, clap::Parser)]
pub struct ListSubcommand {}

fn handle_list(cli: &super::Cli, _cmd: &ListSubcommand) -> anyhow::Result<()> {
    let config = read_config().context("When reading config")?;
    for (name, _) in &config.channels {
        println!("{}", name);
    }

    Ok(())
}

pub fn entry(cli: &super::Cli, cmd: &ToolchainCommandline) -> anyhow::Result<()> {
    match cmd {
        ToolchainCommandline::Default(v) => super::handle_default(cli, v),
        ToolchainCommandline::Install(v) => handle_add(cli, v),
        ToolchainCommandline::Update(v) => handle_update(cli, v),
        ToolchainCommandline::Uninstall(v) => handle_remove(cli, v),
        ToolchainCommandline::List(v) => handle_list(cli, v),
    }
}
