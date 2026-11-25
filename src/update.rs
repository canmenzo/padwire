//! Asks GitHub whether a newer release exists. It never downloads anything: the button opens the release page.
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const PAGE: &str = "https://github.com/canmenzo/padwire/releases/latest";
const API: &str = "https://api.github.com/repos/canmenzo/padwire/releases/latest";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, PartialEq, Debug, Default)]
pub enum Update {
    #[default]
    Unknown,
    Checking,
    Current,
    Newer(String),
    Failed,
}

fn parse(v: &str) -> Option<[u32; 3]> {
    let mut it = v.trim().trim_start_matches('v').split('.').map(|p| p.parse().ok());
    Some([it.next()??, it.next()??, it.next()??])
}

pub fn newer(latest: &str, mine: &str) -> bool {
    matches!((parse(latest), parse(mine)), (Some(a), Some(b)) if a > b)
}

fn latest() -> Option<String> {
    use ureq::tls::{TlsConfig, TlsProvider};
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(8)))
        .tls_config(TlsConfig::builder().provider(TlsProvider::NativeTls).build())
        .build()
        .into();
    let body = agent.get(API).header("User-Agent", concat!("padwire/", env!("CARGO_PKG_VERSION"))).call().ok()?.body_mut().read_to_string().ok()?;
    serde_json::from_str::<serde_json::Value>(&body).ok()?.get("tag_name")?.as_str().map(str::to_string)
}

pub fn check(state: &Arc<Mutex<Update>>) {
    *state.lock().unwrap() = Update::Checking;
    let state = state.clone();
    std::thread::spawn(move || {
        let found = match latest() {
            Some(tag) if newer(&tag, VERSION) => Update::Newer(tag),
            Some(tag) if parse(&tag).is_some() => Update::Current,
            _ => Update::Failed, // no answer, or a tag that is not a version: do not claim to be up to date
        };
        *state.lock().unwrap() = found;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_compare() {
        assert!(newer("v0.1.10", "0.1.9"));
        assert!(newer("v1.0.0", "0.9.9"));
        assert!(!newer("v0.1.3", "0.1.3"));
        assert!(!newer("v0.1.2", "0.1.3"));
        assert!(!newer("nightly", "0.1.3"));
    }

    /// Needs the network: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn github_answers_with_a_release_tag() {
        assert!(parse(&latest().expect("no answer from GitHub")).is_some());
    }
}
