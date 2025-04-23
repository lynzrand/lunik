use std::borrow::Cow;

use crate::channel::Channel;

use super::{Config, ToolchainInfo};

/// An iterator over fallback toolchains choices of a specific toolchain.
pub struct ConfigToolchainFallbackIter<'a> {
    curr_toolchain_name: Option<Cow<'a, str>>,
    config: &'a Config,
}

impl<'a> ConfigToolchainFallbackIter<'a> {
    /// Creates a new iterator over the fallback toolchains of a given toolchain.
    pub fn new(config: &'a Config, toolchain_name: &'a str) -> Self {
        Self {
            curr_toolchain_name: Some(Cow::Borrowed(toolchain_name)),
            config,
        }
    }
}

impl<'a> Iterator for ConfigToolchainFallbackIter<'a> {
    type Item = (Cow<'a, str>, &'a ToolchainInfo);

    fn next(&mut self) -> Option<Self::Item> {
        // Take the current toolchain name to process. If none, iterator is finished.
        let toolchain_name = self.curr_toolchain_name.take()?;

        // Attempt to find the toolchain information
        let (real_toolchain_name, toolchain_info) =
            if let Some(info) = self.config.toolchain.get(toolchain_name.as_ref()) {
                // Found directly
                (toolchain_name, info)
            } else {
                // Not found directly, try parsing as a channel
                match toolchain_name.as_ref().parse::<Channel>() {
                    Ok(ch) => {
                        let real_name = ch.to_string();
                        // Look up using the canonical channel name
                        if let Some(info) = self.config.toolchain.get(&real_name) {
                            (Cow::Owned(real_name), info)
                        } else {
                            // Channel name resolved, but not found in config
                            return None; // Stop iteration if toolchain is definitively not found
                        }
                    }
                    Err(_) => {
                        // Not found directly and not a valid channel format
                        return None; // Stop iteration
                    }
                }
            };

        // Prepare the next toolchain name from the fallback, if it exists.
        self.curr_toolchain_name = toolchain_info.fallback.as_deref().map(Cow::Borrowed);

        // Return the current toolchain name and its info.
        Some((real_toolchain_name, toolchain_info))
    }
}
