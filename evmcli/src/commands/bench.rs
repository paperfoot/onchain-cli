use alloy::primitives::Address;
use alloy::providers::Provider;
use comfy_table::Table;
use serde::{Deserialize, Serialize};
use std::time::Instant;

use crate::context::AppContext;
use crate::errors::EvmError;
use crate::output::table::Tableable;

#[derive(Debug, Serialize)]
pub struct BenchResult {
    pub rpc_endpoint: String,
    pub operations: Vec<OpBench>,
}

#[derive(Debug, Serialize)]
pub struct OpBench {
    pub name: String,
    /// Total measured operations attempted, including failures.
    pub iterations: u32,
    pub successful_iterations: u32,
    pub failed_iterations: u32,
    pub mean_ms: Option<f64>,
    pub p50_ms: Option<f64>,
    pub p95_ms: Option<f64>,
    pub p99_ms: Option<f64>,
    pub min_ms: Option<f64>,
    pub max_ms: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct BlockscoutBenchResponse {
    items: Vec<serde_json::Value>,
}

fn display_timing(timing: Option<f64>) -> String {
    match timing {
        Some(value) => format!("{value:.1}ms"),
        None => "-".to_string(),
    }
}

impl Tableable for BenchResult {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table.set_header(vec![
            "Operation",
            "Attempted",
            "Succeeded",
            "Failed",
            "Mean",
            "P50",
            "P95",
            "P99",
            "Min",
            "Max",
        ]);
        for op in &self.operations {
            table.add_row(vec![
                op.name.clone(),
                op.iterations.to_string(),
                op.successful_iterations.to_string(),
                op.failed_iterations.to_string(),
                display_timing(op.mean_ms),
                display_timing(op.p50_ms),
                display_timing(op.p95_ms),
                display_timing(op.p99_ms),
                display_timing(op.min_ms),
                display_timing(op.max_ms),
            ]);
        }
        table.add_row(vec![
            "RPC",
            &self.rpc_endpoint,
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            "",
        ]);
        table
    }
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((p / 100.0) * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

async fn bench_operation<F, Fut>(name: &str, iterations: u32, warmup: u32, f: F) -> OpBench
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<(), String>>,
{
    // Warmup
    for _ in 0..warmup {
        let _ = f().await;
    }

    // Benchmark — only count successful operations
    let mut timings = Vec::with_capacity(iterations as usize);
    let mut failures = 0u32;
    for _ in 0..iterations {
        let start = Instant::now();
        match f().await {
            Ok(()) => timings.push(start.elapsed().as_secs_f64() * 1000.0),
            Err(_) => failures += 1,
        }
    }

    if timings.is_empty() {
        return OpBench {
            name: name.to_string(),
            iterations,
            successful_iterations: 0,
            failed_iterations: failures,
            mean_ms: None,
            p50_ms: None,
            p95_ms: None,
            p99_ms: None,
            min_ms: None,
            max_ms: None,
        };
    }

    timings.sort_by(f64::total_cmp);

    let mean = timings.iter().sum::<f64>() / timings.len() as f64;
    let successful_iterations = timings.len() as u32;

    OpBench {
        name: name.to_string(),
        iterations,
        successful_iterations,
        failed_iterations: failures,
        mean_ms: Some(mean),
        p50_ms: Some(percentile(&timings, 50.0)),
        p95_ms: Some(percentile(&timings, 95.0)),
        p99_ms: Some(percentile(&timings, 99.0)),
        min_ms: timings.first().copied(),
        max_ms: timings.last().copied(),
    }
}

pub async fn run(
    ctx: &AppContext,
    iterations: u32,
    warmup: u32,
    address: &str,
) -> Result<BenchResult, EvmError> {
    if iterations == 0 {
        return Err(EvmError::validation("--iterations must be >= 1"));
    }
    let addr: Address = address
        .parse()
        .map_err(|_| EvmError::validation(format!("Invalid address: {address}")))?;

    eprintln!(
        "Benchmarking against {} ({} iterations, {} warmup)...",
        crate::rpc::provider::endpoint_label(&ctx.rpc_url),
        iterations,
        warmup
    );

    // Benchmark: get_balance
    let provider = &ctx.provider;
    let balance_bench = bench_operation("balance", iterations, warmup, || async {
        provider
            .get_balance(addr)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
    .await;

    eprintln!("  balance: {} mean", display_timing(balance_bench.mean_ms));

    // Benchmark: get_block_number
    let block_bench = bench_operation("block_number", iterations, warmup, || async {
        provider
            .get_block_number()
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
    .await;

    eprintln!(
        "  block_number: {} mean",
        display_timing(block_bench.mean_ms)
    );

    // Benchmark: gas_price
    let gas_bench = bench_operation("gas_price", iterations, warmup, || async {
        provider
            .get_gas_price()
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
    .await;

    eprintln!("  gas_price: {} mean", display_timing(gas_bench.mean_ms));

    // Benchmark: Blockscout tx list
    let http = ctx.http.clone();
    let explorer_url = format!(
        "{}/addresses/{}/transactions",
        ctx.explorer_v2_url(),
        address
    );
    let explorer_bench =
        bench_operation("blockscout_txs", iterations.min(10), warmup.min(2), || {
            let http = http.clone();
            let url = explorer_url.clone();
            async move {
                let response = http
                    .get(&url)
                    .send()
                    .await
                    .and_then(reqwest::Response::error_for_status)
                    .map_err(|e| e.to_string())?;
                let data: BlockscoutBenchResponse =
                    response.json().await.map_err(|e| e.to_string())?;
                let _ = data.items.len();
                Ok(())
            }
        })
        .await;

    eprintln!(
        "  blockscout_txs: {} mean",
        display_timing(explorer_bench.mean_ms)
    );

    let operations = vec![balance_bench, block_bench, gas_bench, explorer_bench];
    let failed_operations: Vec<_> = operations
        .iter()
        .filter(|operation| operation.successful_iterations == 0)
        .map(|operation| operation.name.as_str())
        .collect();
    if !failed_operations.is_empty() {
        return Err(EvmError::rpc(format!(
            "Benchmark operation(s) had no successful iterations: {}",
            failed_operations.join(", ")
        )));
    }

    Ok(BenchResult {
        rpc_endpoint: crate::rpc::provider::endpoint_label(&ctx.rpc_url),
        operations,
    })
}

#[cfg(test)]
mod tests {
    use super::bench_operation;
    use std::sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    };

    #[tokio::test]
    async fn bench_operation_reports_zero_success_without_timings() {
        let result = bench_operation("failure", 3, 1, || async { Err("failed".to_string()) }).await;

        assert_eq!(result.iterations, 3);
        assert_eq!(result.successful_iterations, 0);
        assert_eq!(result.failed_iterations, 3);
        assert!(result.mean_ms.is_none());
        assert!(result.p50_ms.is_none());
        assert!(result.p95_ms.is_none());
        assert!(result.p99_ms.is_none());
        assert!(result.min_ms.is_none());
        assert!(result.max_ms.is_none());
    }

    #[tokio::test]
    async fn bench_operation_reports_mixed_success_and_failure() {
        let calls = Arc::new(AtomicU32::new(0));
        let result = bench_operation("mixed", 4, 0, || {
            let calls = Arc::clone(&calls);
            async move {
                if calls.fetch_add(1, Ordering::Relaxed).is_multiple_of(2) {
                    Ok(())
                } else {
                    Err("failed".to_string())
                }
            }
        })
        .await;

        assert_eq!(result.iterations, 4);
        assert_eq!(result.successful_iterations, 2);
        assert_eq!(result.failed_iterations, 2);
        assert!(result.mean_ms.is_some());
        assert!(result.p50_ms.is_some());
        assert!(result.p95_ms.is_some());
        assert!(result.p99_ms.is_some());
        assert!(result.min_ms.is_some());
        assert!(result.max_ms.is_some());
    }
}
