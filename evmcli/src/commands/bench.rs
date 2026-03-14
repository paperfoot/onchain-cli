use alloy::primitives::Address;
use alloy::providers::Provider;
use comfy_table::Table;
use serde::Serialize;
use std::time::{Duration, Instant};

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
    pub iterations: u32,
    pub mean_ms: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub min_ms: f64,
    pub max_ms: f64,
}

impl Tableable for BenchResult {
    fn to_table(&self) -> Table {
        let mut table = Table::new();
        table.set_header(vec!["Operation", "Iters", "Mean", "P50", "P95", "P99", "Min", "Max"]);
        for op in &self.operations {
            table.add_row(vec![
                &op.name,
                &op.iterations.to_string(),
                &format!("{:.1}ms", op.mean_ms),
                &format!("{:.1}ms", op.p50_ms),
                &format!("{:.1}ms", op.p95_ms),
                &format!("{:.1}ms", op.p99_ms),
                &format!("{:.1}ms", op.min_ms),
                &format!("{:.1}ms", op.max_ms),
            ]);
        }
        table.add_row(vec!["RPC", &self.rpc_endpoint, "", "", "", "", "", ""]);
        table
    }
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() { return 0.0; }
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
            mean_ms: 0.0, p50_ms: 0.0, p95_ms: 0.0, p99_ms: 0.0, min_ms: 0.0, max_ms: 0.0,
        };
    }

    timings.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let mean = timings.iter().sum::<f64>() / timings.len() as f64;

    OpBench {
        name: name.to_string(),
        iterations,
        mean_ms: mean,
        p50_ms: percentile(&timings, 50.0),
        p95_ms: percentile(&timings, 95.0),
        p99_ms: percentile(&timings, 99.0),
        min_ms: timings.first().copied().unwrap_or(0.0),
        max_ms: timings.last().copied().unwrap_or(0.0),
    }
}

pub async fn run(ctx: &AppContext, iterations: u32, warmup: u32, address: &str) -> Result<BenchResult, EvmError> {
    if iterations == 0 {
        return Err(EvmError::validation("--iterations must be >= 1"));
    }
    let addr: Address = address.parse()
        .map_err(|_| EvmError::validation(format!("Invalid address: {address}")))?;

    eprintln!("Benchmarking against {} ({} iterations, {} warmup)...", ctx.rpc_url, iterations, warmup);

    // Benchmark: get_balance
    let provider = &ctx.provider;
    let balance_bench = bench_operation("balance", iterations, warmup, || async {
        provider.get_balance(addr).await.map(|_| ()).map_err(|e| e.to_string())
    }).await;

    eprintln!("  balance: {:.1}ms mean", balance_bench.mean_ms);

    // Benchmark: get_block_number
    let block_bench = bench_operation("block_number", iterations, warmup, || async {
        provider.get_block_number().await.map(|_| ()).map_err(|e| e.to_string())
    }).await;

    eprintln!("  block_number: {:.1}ms mean", block_bench.mean_ms);

    // Benchmark: gas_price
    let gas_bench = bench_operation("gas_price", iterations, warmup, || async {
        provider.get_gas_price().await.map(|_| ()).map_err(|e| e.to_string())
    }).await;

    eprintln!("  gas_price: {:.1}ms mean", gas_bench.mean_ms);

    // Benchmark: Blockscout tx list
    let http = ctx.http.clone();
    let explorer_url = format!("{}/addresses/{}/transactions", ctx.chain.explorer_v2_url(), address);
    let explorer_bench = bench_operation("blockscout_txs", iterations.min(10), warmup.min(2), || {
        let http = http.clone();
        let url = explorer_url.clone();
        async move {
            http.get(&url).send().await.map(|_| ()).map_err(|e| e.to_string())
        }
    }).await;

    eprintln!("  blockscout_txs: {:.1}ms mean", explorer_bench.mean_ms);

    Ok(BenchResult {
        rpc_endpoint: ctx.rpc_url.clone(),
        operations: vec![balance_bench, block_bench, gas_bench, explorer_bench],
    })
}
