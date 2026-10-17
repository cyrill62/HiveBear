//! Cloud provider API keys, kept in the OS keychain.
//!
//! The UI only ever learns *whether* a key is stored, never its value: keys go
//! in through a password field and are not read back for display.

use crate::error::CmdResult;

pub fn set_cloud_api_key(provider: &str, key: &str) -> CmdResult<()> {
    if provider.is_empty() || provider.len() > 64 {
        return Err("Invalid provider name".into());
    }
    if key.is_empty() || key.len() > 4096 {
        return Err("Invalid API key".into());
    }
    hivebear_core::secrets::set_api_key(provider, key)
}

pub fn delete_cloud_api_key(provider: &str) -> CmdResult<()> {
    hivebear_core::secrets::delete_api_key(provider)
}

/// Which of `providers` have a key stored.
pub fn configured_providers(providers: &[&str]) -> Vec<String> {
    providers
        .iter()
        .filter(|p| hivebear_core::secrets::get_api_key(p).is_some())
        .map(|p| p.to_string())
        .collect()
}

pub fn is_keychain_available() -> bool {
    hivebear_core::secrets::is_keychain_available()
}
