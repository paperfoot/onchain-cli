use comfy_table::Table;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;

use crate::errors::EvmError;
use crate::output::table::Tableable;

const REPOSITORY: &str = "https://github.com/paperfoot/onchain-cli";
const RELEASE_API: &str = "https://api.github.com/repos/paperfoot/onchain-cli/releases/latest";

#[derive(Debug, Serialize)]
pub struct UpdateResult {
    pub current_version: String,
    pub latest_version: String,
    pub updated: bool,
    pub message: String,
    pub status: &'static str,
    pub install_source: &'static str,
    pub update_mode: &'static str,
    pub upgrade_command: Option<String>,
    pub release_url: String,
    pub requires_skill_reinstall: bool,
}

impl Tableable for UpdateResult {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table.add_row(vec!["Current", &self.current_version]);
        table.add_row(vec!["Latest", &self.latest_version]);
        table.add_row(vec!["Status", &self.message]);
        table.add_row(vec!["Installation", self.install_source]);
        if let Some(command) = &self.upgrade_command {
            table.add_row(vec!["Upgrade", command]);
        }
        table
    }
}

#[derive(Clone, Debug, PartialEq)]
enum InstallSource {
    Homebrew,
    Cargo(std::path::PathBuf),
    Unknown,
}

impl InstallSource {
    fn name(&self) -> &'static str {
        match self {
            Self::Homebrew => "homebrew",
            Self::Cargo(_) => "cargo",
            Self::Unknown => "unknown",
        }
    }
    fn command(&self) -> Option<String> {
        match self {
            Self::Homebrew => Some("brew upgrade paperfoot/tap/onchain".into()),
            Self::Cargo(root) => root.to_str().map(|path| {
                let quoted = format!("'{}'", path.replace('\'', "'\"'\"'"));
                format!("cargo install --locked --force --root {quoted} onchain")
            }),
            Self::Unknown => None,
        }
    }
}

fn installation_at(executable: &Path, cargo_root: Option<&Path>) -> InstallSource {
    // A Homebrew receipt is stronger evidence than a path containing "Cellar".
    if let Some(keg) = executable.parent().and_then(Path::parent) {
        let formula = keg.parent();
        let cellar = formula.and_then(Path::parent);
        if formula
            .and_then(Path::file_name)
            .is_some_and(|n| n == "onchain")
            && cellar
                .and_then(Path::file_name)
                .is_some_and(|n| n == "Cellar")
            && std::fs::read(keg.join("INSTALL_RECEIPT.json"))
                .ok()
                .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
                .is_some_and(|receipt| receipt["source"]["tap"] == "paperfoot/tap")
        {
            return InstallSource::Homebrew;
        }
    }
    if let Some(root) = cargo_root {
        if executable.parent() == Some(root.join("bin").as_path()) {
            let metadata = std::fs::read(root.join(".crates2.json"))
                .ok()
                .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
            let owned = metadata
                .as_ref()
                .and_then(|v| v["installs"].as_object())
                .is_some_and(|installs| {
                    installs.iter().any(|(name, record)| {
                        name.starts_with("onchain ")
                            && (name.ends_with(
                                "(registry+https://github.com/rust-lang/crates.io-index)",
                            ) || name.ends_with("(registry+https://index.crates.io/)"))
                            && record["bins"].as_array().is_some_and(|bins| {
                                bins.iter().any(|bin| bin.as_str() == Some("onchain"))
                            })
                    })
                });
            if owned {
                return InstallSource::Cargo(root.to_owned());
            }
        }
    }
    InstallSource::Unknown
}

fn installation() -> InstallSource {
    let Ok(executable) = std::env::current_exe().and_then(std::fs::canonicalize) else {
        return InstallSource::Unknown;
    };
    let cargo_root = std::env::var_os("CARGO_HOME")
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| directories::BaseDirs::new().map(|base| base.home_dir().join(".cargo")))
        .and_then(|path| path.canonicalize().ok());
    let root = executable.parent().and_then(Path::parent);
    let source = installation_at(&executable, root);
    if source != InstallSource::Unknown {
        source
    } else {
        installation_at(&executable, cargo_root.as_deref())
    }
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
}

async fn latest_version(url: &str) -> Result<semver::Version, EvmError> {
    let http = reqwest::Client::builder()
        .user_agent(concat!("onchain/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| EvmError::config("Could not initialize release client"))?;
    let mut response = http.get(url).send().await.map_err(|_| {
        EvmError::rpc("Could not check GitHub releases; check connectivity and try update --check")
    })?;
    if !response.status().is_success() {
        return Err(EvmError::rpc(format!(
            "Release lookup returned HTTP {}; check GitHub availability or rate limits",
            response.status().as_u16()
        )));
    }
    const MAX_BODY: usize = 1_048_576;
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| EvmError::rpc("Release response interrupted"))?
    {
        if bytes.len().saturating_add(chunk.len()) > MAX_BODY {
            return Err(EvmError::rpc("Release response exceeds 1 MiB"));
        }
        bytes.extend_from_slice(&chunk);
    }
    let release: Release = serde_json::from_slice(&bytes)
        .map_err(|_| EvmError::rpc("GitHub returned invalid release metadata"))?;
    let version = semver::Version::parse(
        release
            .tag_name
            .strip_prefix('v')
            .unwrap_or(&release.tag_name),
    )
    .map_err(|_| EvmError::rpc("Release has an invalid version"))?;
    if release.draft || release.prerelease || !version.pre.is_empty() || !version.build.is_empty() {
        return Err(EvmError::rpc("Latest release is not a stable version"));
    }
    Ok(version)
}

fn result_for(latest: semver::Version, source: InstallSource, check_only: bool) -> UpdateResult {
    let current = env!("CARGO_PKG_VERSION");
    let available = latest > semver::Version::parse(current).expect("Cargo package version");
    let command = source.command();
    UpdateResult {
        current_version: current.into(),
        latest_version: latest.to_string(),
        updated: false,
        message: if !available {
            "Already up to date".into()
        } else if let Some(command) = &command {
            format!("Update available. Run '{command}' to upgrade this installation.")
        } else {
            format!("Update available. Install the matching verified release from {REPOSITORY}/releases; installation ownership is unknown.")
        },
        status: if !available {
            "up_to_date"
        } else if !check_only && command.is_some() {
            "managed_install"
        } else {
            "update_available"
        },
        install_source: source.name(),
        update_mode: if command.is_some() {
            "package_manager"
        } else {
            "instructions_only"
        },
        upgrade_command: command,
        release_url: format!("{REPOSITORY}/releases/tag/v{latest}"),
        requires_skill_reinstall: available,
    }
}

pub async fn run(check_only: bool) -> Result<UpdateResult, EvmError> {
    Ok(result_for(
        latest_version(RELEASE_API).await?,
        installation(),
        check_only,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};

    #[test]
    fn updates_preserve_ownership_and_never_downgrade() {
        for source in [
            InstallSource::Cargo(std::path::PathBuf::from("/tmp/cargo root")),
            InstallSource::Homebrew,
            InstallSource::Unknown,
        ] {
            for check in [false, true] {
                let result = result_for(
                    semver::Version::parse("999.0.0").unwrap(),
                    source.clone(),
                    check,
                );
                assert!(!result.updated);
                assert_eq!(result.upgrade_command, source.command());
                assert_eq!(result.install_source, source.name());
                assert_ne!(result.update_mode, "self_replace");
                let older = result_for(
                    semver::Version::parse("0.1.0").unwrap(),
                    source.clone(),
                    check,
                );
                assert_eq!(older.status, "up_to_date");
                assert!(!older.requires_skill_reinstall);
            }
        }
        assert_eq!(
            installation_at(Path::new("/tmp/target/release/onchain"), None),
            InstallSource::Unknown
        );
        assert_eq!(
            installation_at(Path::new("/tmp/Cellar/onchain/0.2.1/bin/onchain"), None),
            InstallSource::Unknown
        );
    }

    #[test]
    fn cargo_upgrade_preserves_and_quotes_custom_roots() {
        let root = "/tmp/custom ' $(printf injected) install";
        let source = InstallSource::Cargo(std::path::PathBuf::from(root));
        let command = source.command().unwrap();
        // Parse the suggestion as shell arguments without invoking Cargo.
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("set -- {command}; printf '%s' \"$6\""))
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(String::from_utf8(output.stdout).unwrap(), root);
    }

    #[test]
    fn ownership_requires_package_manager_receipts() {
        let temp = std::env::temp_dir().join(format!("onchain-ownership-{}", std::process::id()));
        let keg = temp.join("Cellar/onchain/0.2.1");
        std::fs::create_dir_all(keg.join("bin")).unwrap();
        let executable = keg.join("bin/onchain");
        assert_eq!(installation_at(&executable, None), InstallSource::Unknown);
        std::fs::write(
            keg.join("INSTALL_RECEIPT.json"),
            r#"{"source":{"tap":"paperfoot/tap"}}"#,
        )
        .unwrap();
        assert_eq!(installation_at(&executable, None), InstallSource::Homebrew);
        std::fs::write(
            keg.join("INSTALL_RECEIPT.json"),
            r#"{"source":{"tap":"another/tap"}}"#,
        )
        .unwrap();
        assert_eq!(installation_at(&executable, None), InstallSource::Unknown);
        let cargo = temp.join("cargo");
        std::fs::create_dir_all(cargo.join("bin")).unwrap();
        let executable = cargo.join("bin/onchain");
        assert_eq!(
            installation_at(&executable, Some(&cargo)),
            InstallSource::Unknown
        );
        std::fs::write(cargo.join(".crates2.json"), r#"{"installs":{"onchain 0.2.1 (registry+https://github.com/rust-lang/crates.io-index)":{"bins":["onchain"]}}}"#).unwrap();
        assert_eq!(
            installation_at(&executable, Some(&cargo)),
            InstallSource::Cargo(cargo.clone())
        );
        std::fs::write(
            cargo.join(".crates2.json"),
            r#"{"installs":{"onchain 0.2.1 (path+file:///src)":{"bins":["onchain"]}}}"#,
        )
        .unwrap();
        assert_eq!(
            installation_at(&executable, Some(&cargo)),
            InstallSource::Unknown
        );
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[tokio::test]
    async fn release_checks_validate_provider_success_version_and_body() {
        let server = MockServer::start().await;
        for (body, valid) in [
            (
                r#"{"tag_name":"v0.10.0","draft":false,"prerelease":false}"#,
                true,
            ),
            (
                r#"{"tag_name":"invalid","draft":false,"prerelease":false}"#,
                false,
            ),
            (
                r#"{"tag_name":"v2.0.0-rc.1","draft":false,"prerelease":false}"#,
                false,
            ),
            (
                r#"{"tag_name":"v2.0.0","draft":true,"prerelease":false}"#,
                false,
            ),
            ("{}", false),
        ] {
            server.reset().await;
            Mock::given(method("GET"))
                .respond_with(ResponseTemplate::new(200).set_body_string(body))
                .mount(&server)
                .await;
            assert_eq!(latest_version(&server.uri()).await.is_ok(), valid);
        }
        for response in [
            ResponseTemplate::new(429),
            ResponseTemplate::new(200).set_body_string("x".repeat(1_048_577)),
        ] {
            server.reset().await;
            Mock::given(method("GET"))
                .respond_with(response)
                .mount(&server)
                .await;
            assert!(latest_version(&server.uri()).await.is_err());
        }
    }
}
