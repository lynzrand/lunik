//! Information of release channels.

use std::str::FromStr;

/// Represents a release channel.
///
/// Format: `<channel>[-<host>]`, where:
///
/// - `<channel>` is either `latest` or a version number.
/// - `<host>` is `<os>-<arch>`.
#[derive(Debug)]
pub struct Channel {
    pub channel: ChannelKind,
    pub host: Host,
}

impl FromStr for Channel {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.splitn(2, '-');
        let channel = parts
            .next()
            .ok_or_else(|| anyhow::anyhow!("missing channel"))?;
        let channel = ChannelKind::from_str(channel)?;
        let host = parts
            .next()
            .map(Host::from_str)
            .unwrap_or_else(|| Ok(Host::default()))?;
        Ok(Channel { channel, host })
    }
}

impl std::fmt::Display for Channel {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}-{}", self.channel, self.host)
    }
}

impl Default for Channel {
    fn default() -> Self {
        Channel {
            channel: ChannelKind::Latest,
            host: Host::default(),
        }
    }
}

#[derive(Debug)]
pub enum ChannelKind {
    /// Latest public release.
    Latest,
    /// Bleeding edge release directly from CI.
    Bleeding,
    /// A specific version.
    Version(String),
}

impl FromStr for ChannelKind {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "latest" => Ok(ChannelKind::Latest),
            "bleeding" => Ok(ChannelKind::Bleeding),
            _ => Ok(ChannelKind::Version(s.to_string())),
        }
    }
}

impl std::fmt::Display for ChannelKind {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ChannelKind::Latest => write!(f, "latest"),
            ChannelKind::Bleeding => write!(f, "bleeding"),
            ChannelKind::Version(v) => write!(f, "{}", v),
        }
    }
}

#[derive(Debug)]
pub struct Host {
    os: String,
    arch: String,
}

impl FromStr for Host {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.split('-');
        let os = parts.next().ok_or_else(|| anyhow::anyhow!("missing os"))?;
        let arch = parts
            .next()
            .ok_or_else(|| anyhow::anyhow!("missing arch"))?;
        Ok(Host {
            os: os.to_string(),
            arch: arch.to_string(),
        })
    }
}

impl Default for Host {
    fn default() -> Self {
        Host {
            os: default_os_string().to_string(),
            arch: default_arch_string().to_string(),
        }
    }
}

impl std::fmt::Display for Host {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}-{}", self.os, self.arch)
    }
}

pub fn default_os_string() -> &'static str {
    match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    }
}

pub fn default_arch_string() -> &'static str {
    std::env::consts::ARCH
}

#[cfg(test)]
mod test {
    #[test]
    fn test_roundtrip() {
        let current_host = super::Host::default();

        let ch = "latest".parse::<super::Channel>().unwrap();
        assert_eq!(ch.to_string(), format!("latest-{}", current_host));

        let ch = "bleeding".parse::<super::Channel>().unwrap();
        assert_eq!(ch.to_string(), format!("bleeding-{}", current_host));

        let ch = "1.0.0".parse::<super::Channel>().unwrap();
        assert_eq!(ch.to_string(), format!("1.0.0-{}", current_host));

        let ch = "latest-linux-x86_64".parse::<super::Channel>().unwrap();
        assert_eq!(ch.to_string(), "latest-linux-x86_64");

        let ch = "bleeding-linux-x86_64".parse::<super::Channel>().unwrap();
        assert_eq!(ch.to_string(), "bleeding-linux-x86_64");

        let ch = "1.0.0-linux-x86_64".parse::<super::Channel>().unwrap();
        assert_eq!(ch.to_string(), "1.0.0-linux-x86_64");
    }

    #[test]
    fn test_malformed() {
        assert!("".parse::<super::Channel>().is_err());
        assert!("latest-".parse::<super::Channel>().is_err());
        assert!("latest-linux".parse::<super::Channel>().is_err());
    }
}
