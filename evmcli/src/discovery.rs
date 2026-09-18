//! Offline, parser-derived capability discovery for agents.
//!
//! Syntax is read from the same Clap command tree used by the executable.  The
//! semantic annotations below are deliberately conservative: every currently
//! exposed operation is a read (including dry swap quotes), and the legacy
//! output and exit-code contracts are described rather than overstated.

use crate::{cli::Cli, errors::EvmError};
use clap::{Arg, ArgAction, Command, CommandFactory};
use serde_json::{json, Map, Value};
use std::collections::HashSet;

fn is_presentation_arg(arg: &Arg) -> bool {
    arg.is_hide_set()
        || matches!(
            arg.get_action(),
            ArgAction::Help | ArgAction::HelpShort | ArgAction::HelpLong | ArgAction::Version
        )
}

fn value_type(path: &str, arg: &Arg) -> &'static str {
    if matches!(arg.get_action(), ArgAction::SetTrue | ArgAction::SetFalse) {
        return "bool";
    }
    match (path, arg.get_id().as_str()) {
        (_, "cookie_file") | ("zcash batch", "file") => "path",
        ("zcash amount", "zec") => "decimal_string",
        ("swap quote", "amount") => "unsigned_integer_string",
        (
            _,
            "pages"
            | "from_block"
            | "to_block"
            | "block"
            | "iterations"
            | "warmup"
            | "max_tip_age_seconds"
            | "max_lag_blocks"
            | "actions"
            | "timeout_ms"
            | "slippage_bps"
            | "deadline_minutes",
        ) => "unsigned_integer",
        _ => "string",
    }
}

fn numeric_range(path: &str, id: &str) -> Option<(u64, Option<u64>)> {
    match (path, id) {
        ("txs" | "transfers", "pages") => Some((1, Some(100))),
        ("bench", "iterations") => Some((1, None)),
        ("zcash health", "max_tip_age_seconds") => Some((1, None)),
        (
            "zcash info" | "zcash health" | "zcash balance" | "zcash utxos" | "zcash tx"
            | "zcash block" | "zcash mempool" | "zcash batch" | "zcash amount" | "zcash fee"
            | "zcash bench",
            "timeout_ms",
        ) => Some((100, Some(120_000))),
        ("zcash bench", "iterations") => Some((1, Some(1_000))),
        ("swap tokens" | "swap quote" | "swap status", "timeout_ms") => Some((100, Some(120_000))),
        ("swap quote", "slippage_bps") => Some((0, Some(5_000))),
        ("swap quote", "deadline_minutes") => Some((5, Some(1_440))),
        _ => None,
    }
}

fn conflicts(path: &str, id: &str) -> &'static [&'static str] {
    match (path, id) {
        ("logs", "topic0") => &["--event"],
        ("logs", "event") => &["--topic0"],
        _ => &[],
    }
}

fn argument(path: &str, arg: &Arg) -> Value {
    let boolean = matches!(arg.get_action(), ArgAction::SetTrue | ArgAction::SetFalse);
    let positional = arg.get_index().is_some();
    let mut value = json!({
        "name": arg
            .get_long()
            .map(|name| format!("--{name}"))
            .unwrap_or_else(|| arg.get_id().to_string()),
        "type": value_type(path, arg),
        "required": arg.is_required_set(),
        "description": arg.get_help().map(ToString::to_string).unwrap_or_default(),
    });

    if positional {
        value["kind"] = json!("positional");
    }
    if let Some(short) = arg.get_short() {
        value["short"] = json!(format!("-{short}"));
    }
    let defaults = arg.get_default_values();
    if defaults.len() == 1 {
        let default = defaults[0].to_string_lossy();
        value["default"] = if boolean {
            json!(default == "true")
        } else {
            json!(default)
        };
    } else if !defaults.is_empty() {
        value["default"] = json!(defaults
            .iter()
            .map(|item| item.to_string_lossy())
            .collect::<Vec<_>>());
    }
    if let Some(values) = arg
        .get_value_parser()
        .possible_values()
        .filter(|_| !boolean)
    {
        value["values"] = values
            .filter(|possible| !possible.is_hide_set())
            .map(|possible| Value::String(possible.get_name().to_owned()))
            .collect();
    }
    if let Some(arity) = arg.get_num_args() {
        let min = arity.min_values();
        let max = arity.max_values();
        if min != 1 || max != 1 {
            value["arity"] = json!({
                "min": min,
                "max": (max != usize::MAX).then_some(max),
            });
        }
    }
    if let Some(environment) = arg.get_env() {
        value["env"] = json!(environment.to_string_lossy());
    }
    if let Some((minimum, maximum)) = numeric_range(path, arg.get_id().as_str()) {
        value["minimum"] = json!(minimum);
        if let Some(maximum) = maximum {
            value["maximum"] = json!(maximum);
        }
    }
    let conflicts = conflicts(path, arg.get_id().as_str());
    if !conflicts.is_empty() {
        value["conflicts_with"] = json!(conflicts);
    }
    value
}

fn annotations(path: &str) -> Option<Value> {
    let example: &[&str] = match path {
        "agent-info" => &["agent-info", "--command", "gas"],
        "zcash info" => &["zcash", "info"],
        "zcash health" => &["zcash", "health", "--max-lag-blocks", "2"],
        "zcash balance" => &["zcash", "balance", "t3dvVE3SQEi7kqNzwrfNePxZ1d4hUyztBA1"],
        "zcash utxos" => &["zcash", "utxos", "t3dvVE3SQEi7kqNzwrfNePxZ1d4hUyztBA1"],
        "zcash tx" => &[
            "zcash",
            "tx",
            "0000000000000000000000000000000000000000000000000000000000000000",
        ],
        "zcash block" => &["zcash", "block", "latest"],
        "zcash mempool" => &["zcash", "mempool"],
        "zcash batch" => &["zcash", "batch", "requests.json"],
        "zcash amount" => &["zcash", "amount", "1.00000000"],
        "zcash fee" => &["zcash", "fee", "--actions", "2"],
        "zcash bench" => &["zcash", "bench", "--iterations", "10"],
        "swap tokens" => &["swap", "tokens", "--chain", "zec"],
        "swap quote" => &[
            "swap",
            "quote",
            "--from",
            "nep141:zec.omft.near",
            "--to",
            "nep141:usdc.example",
            "--amount",
            "100000000",
            "--recipient",
            "destination",
            "--refund-to",
            "refund",
        ],
        "swap status" => &["swap", "status", "deposit-address"],
        "balance" => &["balance", "0x0000000000000000000000000000000000000000"],
        "tx" => &[
            "tx",
            "0x0000000000000000000000000000000000000000000000000000000000000000",
        ],
        "receipt" => &[
            "receipt",
            "0x0000000000000000000000000000000000000000000000000000000000000000",
        ],
        "block" => &["block", "latest"],
        "gas" => &["gas"],
        "call" => &[
            "call",
            "0x0000000000000000000000000000000000000000",
            "owner()(address)",
        ],
        "txs" => &[
            "txs",
            "0x0000000000000000000000000000000000000000",
            "--pages",
            "1",
        ],
        "decode" => &["decode", "0x12345678"],
        "abi" => &["abi", "0x0000000000000000000000000000000000000000"],
        "logs" => &["logs", "--event", "transfer"],
        "transfers" => &["transfers", "0x0000000000000000000000000000000000000000"],
        "storage" => &[
            "storage",
            "0x0000000000000000000000000000000000000000",
            "0x0",
        ],
        "nonce" => &["nonce", "0x0000000000000000000000000000000000000000"],
        "code" => &["code", "0x0000000000000000000000000000000000000000"],
        "trace" => &[
            "trace",
            "0x0000000000000000000000000000000000000000000000000000000000000000",
        ],
        "bench" => &["bench", "--iterations", "1", "--warmup", "0"],
        "update" => &["update", "--check"],
        "examples" => &["examples"],
        _ => return None,
    };
    let mut metadata = json!({
        "effect": "read",
        "idempotent": true,
        "examples": [example],
    });
    if path == "swap quote" {
        metadata["effect_detail"] = json!(
            "Requests a dry preview only; it does not create a deposit address or move funds"
        );
    } else if path == "update" {
        metadata["effect_detail"] = json!(
            "Returns checked release status or installation instructions; it does not replace the binary"
        );
    }
    Some(metadata)
}

fn command_arguments(command: &Command, root_globals: &HashSet<String>) -> Vec<Arg> {
    command
        .get_arguments()
        .filter(|arg| !is_presentation_arg(arg))
        .filter(|arg| !root_globals.contains(arg.get_id().as_str()))
        .cloned()
        .collect()
}

fn commands(
    command: &Command,
    prefix: &str,
    root_globals: &HashSet<String>,
    inherited: &[Arg],
    result: &mut Map<String, Value>,
) -> Result<(), EvmError> {
    for child in command.get_subcommands().filter(|item| !item.is_hide_set()) {
        if child.get_name() == "help" {
            continue;
        }
        let path = format!("{prefix}{}", child.get_name());
        let mut available = inherited.to_vec();
        let mut seen: HashSet<String> = available
            .iter()
            .map(|arg| arg.get_id().to_string())
            .collect();
        for arg in command_arguments(child, root_globals) {
            if seen.insert(arg.get_id().to_string()) {
                available.push(arg);
            }
        }

        if child.get_subcommands().any(|item| !item.is_hide_set()) {
            commands(child, &format!("{path} "), root_globals, &available, result)?;
            continue;
        }

        let mut args = Vec::new();
        let mut options = Vec::new();
        for arg in &available {
            if arg.get_index().is_some() {
                args.push(argument(&path, arg));
            } else {
                options.push(argument(&path, arg));
            }
        }
        let mut entry = json!({
            "description": child
                .get_about()
                .map(ToString::to_string)
                .unwrap_or_default(),
            "args": args,
            "options": options,
        });
        let aliases: Vec<_> = child.get_visible_aliases().collect();
        if !aliases.is_empty() {
            entry["aliases"] = json!(aliases);
        }
        let annotation = annotations(&path).ok_or_else(|| {
            EvmError::config(format!("Discovery metadata is missing command: {path}"))
        })?;
        entry
            .as_object_mut()
            .expect("command entry is an object")
            .extend(
                annotation
                    .as_object()
                    .expect("command annotation is an object")
                    .clone(),
            );
        result.insert(path, entry);
    }
    Ok(())
}

fn environment_metadata() -> Value {
    json!({
        "ONCHAIN_NETWORK": {"secret": false, "description": "Default EVM network name or chain ID"},
        "ONCHAIN_RPC_URL": {"secret": true, "description": "Custom EVM RPC endpoint"},
        "ONCHAIN_EXPLORER_URL": {"secret": true, "description": "Custom Blockscout API endpoint"},
        "ONCHAIN_TRACE_RPC_URL": {"secret": true, "description": "Optional tracing-capable EVM RPC endpoint"},
        "ONCHAIN_ZCASH_RPC_URL": {"secret": true, "description": "Custom native Zcash RPC endpoint"},
        "ONCHAIN_ZCASH_NETWORK": {"secret": false, "description": "Native Zcash network"},
        "ONCHAIN_ZCASH_COOKIE_FILE": {"secret": true, "description": "Path to a Zcash RPC authentication cookie"},
        "ONCHAIN_ZCASH_RPC_USER": {"secret": true, "description": "Zcash RPC username"},
        "ONCHAIN_ZCASH_RPC_PASSWORD": {"secret": true, "description": "Zcash RPC password"},
        "ONCHAIN_SWAP_API_URL": {"secret": true, "description": "NEAR Intents 1Click API endpoint"},
        "ONCHAIN_SWAP_API_KEY": {"secret": true, "description": "NEAR Intents 1Click API key"},
        "ONCHAIN_SWAP_JWT": {"secret": true, "description": "NEAR Intents 1Click bearer token"},
        "RUST_LOG": {"secret": false, "description": "Tracing filter; diagnostic logs are written to stderr"}
    })
}

/// Build the complete or command-scoped capability manifest without loading
/// configuration, credentials, or network clients.
pub fn manifest(filter: Option<&str>) -> Result<Value, EvmError> {
    let mut root = Cli::command();
    root.build();

    let root_globals: HashSet<String> = root
        .get_arguments()
        .filter(|arg| arg.is_global_set() && !is_presentation_arg(arg))
        .map(|arg| arg.get_id().to_string())
        .collect();
    let mut entries = Map::new();
    commands(&root, "", &root_globals, &[], &mut entries)?;

    if let Some(filter) = filter {
        let canonical = filter.split_whitespace().collect::<Vec<_>>().join(" ");
        if canonical.is_empty() {
            return Err(EvmError::validation("Command path cannot be empty"));
        }
        let prefix = format!("{canonical} ");
        entries.retain(|path, _| path == &canonical || path.starts_with(&prefix));
        if entries.is_empty() {
            return Err(EvmError::validation(format!(
                "Unknown command path: {canonical}. Use a canonical path from 'onchain agent-info'"
            )));
        }
    }

    let global_flags = root
        .get_arguments()
        .filter(|arg| arg.is_global_set() && !is_presentation_arg(arg))
        .map(|arg| {
            let mut value = argument("", arg);
            let name = value
                .as_object_mut()
                .expect("global flag metadata is an object")
                .remove("name")
                .expect("global flag has a name")
                .as_str()
                .expect("global flag name is text")
                .to_owned();
            (name, value)
        })
        .collect::<Map<_, _>>();

    Ok(json!({
        "name": env!("CARGO_PKG_NAME"),
        "version": env!("CARGO_PKG_VERSION"),
        "description": env!("CARGO_PKG_DESCRIPTION"),
        "commands": entries,
        "global_flags": global_flags,
        "exit_codes": {
            "0": "Success, help, or version",
            "1": "Validation, explorer, ABI, decode, serialization, or other command failure",
            "2": "Configuration failure or parser usage error",
            "3": "EVM or Zcash RPC failure",
            "5": "Signing failure"
        },
        "output": {
            "auto_json_when_piped": true,
            "json_flag": "--json",
            "success": "Raw command-specific JSON without an envelope",
            "json_errors": "Raw {error,message} JSON is written to stdout",
            "human_errors": "Human-readable errors are written to stderr",
            "framework_envelope": false
        },
        "config": {
            "supported": false,
            "path": null,
            "precedence": ["command_line", "environment", "built_in_defaults"]
        },
        "env": environment_metadata()
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn option<'a>(manifest: &'a Value, command: &str, name: &str) -> &'a Value {
        manifest["commands"][command]["options"]
            .as_array()
            .unwrap()
            .iter()
            .find(|option| option["name"] == name)
            .unwrap_or_else(|| panic!("missing {name} on {command}"))
    }

    #[test]
    fn every_public_leaf_has_semantics_and_a_parsing_example() {
        let manifest = manifest(None).unwrap();
        let commands = manifest["commands"].as_object().unwrap();
        assert_eq!(commands.len(), 33);
        for (path, command) in commands {
            assert_eq!(command["effect"], "read", "{path}");
            assert_eq!(command["idempotent"], true, "{path}");
            for example in command["examples"].as_array().unwrap() {
                let argv = std::iter::once("onchain").chain(
                    example
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|part| part.as_str().unwrap()),
                );
                assert!(
                    Cli::try_parse_from(argv).is_ok(),
                    "bad example for {path}: {example}"
                );
            }
        }
    }

    #[test]
    fn filters_change_only_commands_and_groups_select_descendants() {
        let mut full = manifest(None).unwrap();
        let mut scoped = manifest(Some("  zcash   ")).unwrap();
        let scoped_commands = scoped["commands"].as_object().unwrap();
        assert_eq!(scoped_commands.len(), 11);
        assert!(scoped_commands
            .keys()
            .all(|path| path.starts_with("zcash ")));
        full.as_object_mut().unwrap().remove("commands");
        scoped.as_object_mut().unwrap().remove("commands");
        assert_eq!(scoped, full);

        let leaf = manifest(Some("swap quote")).unwrap();
        assert_eq!(leaf["commands"].as_object().unwrap().len(), 1);
        assert!(leaf["commands"].get("swap quote").is_some());
    }

    #[test]
    fn rejects_empty_unknown_and_alias_filters() {
        for filter in ["", "missing", "info"] {
            assert!(matches!(
                manifest(Some(filter)),
                Err(EvmError::Validation { .. })
            ));
        }
    }

    #[test]
    fn nested_commands_include_ancestor_options_but_not_root_globals() {
        let manifest = manifest(None).unwrap();
        for name in [
            "--zcash-rpc-url",
            "--zcash-network",
            "--cookie-file",
            "--timeout-ms",
        ] {
            assert!(option(&manifest, "zcash info", name).is_object());
        }
        for name in ["--api-url", "--timeout-ms"] {
            assert!(option(&manifest, "swap tokens", name).is_object());
        }
        assert!(manifest["commands"]["zcash info"]["options"]
            .as_array()
            .unwrap()
            .iter()
            .all(|option| option["name"] != "--json"));
    }

    #[test]
    fn reports_defaults_enums_ranges_conflicts_aliases_and_arity() {
        let manifest = manifest(None).unwrap();
        assert_eq!(
            option(&manifest, "zcash info", "--zcash-network")["default"],
            "mainnet"
        );
        assert_eq!(
            option(&manifest, "zcash info", "--zcash-network")["values"],
            json!(["mainnet", "testnet", "regtest"])
        );
        assert_eq!(option(&manifest, "txs", "--pages")["minimum"], 1);
        assert_eq!(option(&manifest, "txs", "--pages")["maximum"], 100);
        assert_eq!(
            option(&manifest, "logs", "--topic0")["conflicts_with"],
            json!(["--event"])
        );
        assert_eq!(
            manifest["commands"]["agent-info"]["aliases"],
            json!(["info"])
        );
        let call_args = manifest["commands"]["call"]["args"].as_array().unwrap();
        assert_eq!(call_args.last().unwrap()["name"], "args");
        assert_eq!(
            call_args.last().unwrap()["arity"],
            json!({"min": 1, "max": null})
        );
    }

    #[test]
    fn environment_metadata_never_contains_runtime_values() {
        let manifest = manifest(None).unwrap();
        for (name, metadata) in manifest["env"].as_object().unwrap() {
            assert!(name.starts_with("ONCHAIN_") || name == "RUST_LOG");
            assert!(metadata.get("value").is_none());
            assert!(metadata["secret"].is_boolean());
            assert!(metadata["description"]
                .as_str()
                .is_some_and(|text| !text.is_empty()));
        }
        assert_eq!(manifest["output"]["framework_envelope"], false);
        assert_eq!(manifest["config"]["supported"], false);
    }
}
