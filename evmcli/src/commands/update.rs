use comfy_table::Table;
use serde::Serialize;

use crate::errors::EvmError;
use crate::output::table::Tableable;

#[derive(Debug, Serialize)]
pub struct UpdateResult {
    pub current_version: String,
    pub latest_version: String,
    pub updated: bool,
    pub message: String,
}

impl Tableable for UpdateResult {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table.add_row(vec!["Current", &self.current_version]);
        table.add_row(vec!["Latest", &self.latest_version]);
        table.add_row(vec!["Status", &self.message]);
        table
    }
}

pub async fn run(check_only: bool) -> Result<UpdateResult, EvmError> {
    // The updater uses blocking HTTP; keep its runtime outside Tokio's async workers.
    tokio::task::spawn_blocking(move || run_blocking(check_only))
        .await
        .map_err(|e| EvmError::config(format!("Updater failed: {e}")))?
}

fn newer(latest: &str, current: &str) -> Result<bool, EvmError> {
    let parse = |value: &str| {
        semver::Version::parse(value.trim_start_matches('v'))
            .map_err(|_| EvmError::config("Release has an invalid version"))
    };
    Ok(parse(latest)? > parse(current)?)
}

fn run_blocking(check_only: bool) -> Result<UpdateResult, EvmError> {
    let current = env!("CARGO_PKG_VERSION");
    let updater = self_update::backends::github::Update::configure()
        .repo_owner("paperfoot")
        .repo_name("onchain-cli")
        .bin_name("onchain")
        .current_version(current)
        .unattended()
        .checksum_from_asset("SHA256SUMS")
        .build()
        .map_err(|e| EvmError::config(format!("Update configuration failed: {e}")))?;
    let releases = updater
        .get_latest_release()
        .map_err(|e| EvmError::config(format!("Could not check for updates: {e}")))?;
    let latest = releases
        .latest()
        .ok_or_else(|| EvmError::config("No published releases found"))?;
    let latest_version = latest.version().to_string();
    let mut result = UpdateResult {
        current_version: current.to_string(),
        latest_version: latest_version.clone(),
        updated: false,
        message: "Already up to date".into(),
    };
    if !newer(&latest_version, current)? {
        return Ok(result);
    }
    if check_only {
        result.message = "Update available. Run 'onchain update' to install.".into();
        return Ok(result);
    }
    let status = updater
        .update()
        .map_err(|e| EvmError::config(format!("Update failed: {e}")))?;
    result.updated = status.is_updated();
    result.latest_version = status.version().to_string();
    result.message = if result.updated {
        format!("Updated to {}", status.version())
    } else {
        "Already up to date".into()
    };
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn updates_compare_semantic_versions_without_downgrades() {
        assert!(newer("v0.10.0", "0.2.0").unwrap());
        assert!(!newer("0.1.9", "0.2.0").unwrap());
        assert!(!newer("v0.2.0", "0.2.0").unwrap());
        assert!(newer("invalid", "0.2.0").is_err());
    }
}
