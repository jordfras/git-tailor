// Copyright 2026 Thomas Johannesson
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! The one crates.io request the update check makes.

use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use ureq::Agent;
use ureq::tls::{RootCerts, TlsConfig};

const API: &str = "https://crates.io/api/v1/crates";

/// crates.io answers requests without a descriptive User-Agent with 403.
const USER_AGENT: &str = concat!(
    "git-tailor/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/jordfras/git-tailor)"
);

/// Short enough that a black-holed connection cannot keep the thread alive
/// anywhere near as long as a session lasts.
const TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Deserialize)]
struct Response {
    #[serde(rename = "crate")]
    krate: CrateInfo,
}

#[derive(Deserialize)]
struct CrateInfo {
    max_stable_version: Option<String>,
}

/// The newest stable release of `crate_name` on crates.io.
pub fn latest_stable_version(crate_name: &str) -> Result<String> {
    // `PlatformVerifier` validates against the operating system's trust store
    // instead of ureq's compiled-in Mozilla roots. A TLS-inspecting corporate
    // proxy signs with a private CA that is only ever installed system-wide, so
    // the Mozilla roots reject it and the check fails with `UnknownIssuer`.
    let agent: Agent = Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .tls_config(
            TlsConfig::builder()
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
        .build()
        .into();

    let url = format!("{API}/{crate_name}");
    let response: Response = agent
        .get(&url)
        .header("User-Agent", USER_AGENT)
        .call()
        .with_context(|| format!("requesting {url}"))?
        .body_mut()
        .read_json()
        .context("parsing the crates.io response")?;

    response
        .krate
        .max_stable_version
        .ok_or_else(|| anyhow!("crates.io reports no stable release of {crate_name}"))
}
