use anyhow::{Context, Result};
use clap::Parser;
use log::{error, info, warn};
use rotator_rs::types::{Proxy, RotationDecision};
use rotator_rs::{polish, rotator, tunnel, verifier};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

fn init_logging() {
    let fmt_layer = fmt::layer()
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true);

    let filter_layer = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))
        .unwrap();

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt_layer)
        .init();
}

#[derive(Parser)]
#[command(name = "spectre")]
#[command(about = "Spectre Network Orchestrator", long_about = None)]
struct Cli {
    #[arg(long, default_value = "phantom")]
    mode: String,

    #[arg(long, default_value_t = 500)]
    limit: usize,

    #[arg(long, default_value = "all")]
    protocol: String,

    #[arg(long, default_value = "full")]
    step: String,

    #[arg(long)]
    stats: bool,

    #[arg(long, default_value_t = 1080)]
    port: u16,

    /// Skip pool re-verification and always scrape fresh proxies
    #[arg(long)]
    force_scrape: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    init_logging();

    let cli = Cli::parse();
    let workspace = std::env::current_dir()?;

    if cli.stats {
        print_stats(&workspace)?;
        return Ok(());
    }

    match cli.step.as_str() {
        "scrape" => {
            run_scraper(&workspace, cli.limit, &cli.protocol)?;
        }
        "polish" => {
            let raw = load_proxies(&workspace.join("raw_proxies.json"))?;
            run_polish(&workspace, raw)?;
        }
        "rotate" => {
            let (dns, non_dns, combined) = load_pools(&workspace)?;
            let decision = rotator::build_chain_decision(&cli.mode, &dns, &non_dns, &combined);
            if let Some(d) = decision {
                print_decision(&d);
            } else {
                error!("Failed to build chain");
            }
        }
        "serve" => {
            let (dns, non_dns, combined) = load_pools(&workspace)?;
            let decision = rotator::build_chain_decision(&cli.mode, &dns, &non_dns, &combined);
            if let Some(d) = decision {
                print_decision(&d);
                tunnel::start_socks_server(cli.port, d, dns, non_dns, combined).await?;
            } else {
                error!("Failed to build chain. Run 'full' or 'scrape' first to populate pools.");
            }
        }
        "refresh" => {
            // Load existing pool, re-verify, fill delta if needed
            let combined = load_proxies(&workspace.join("proxies_combined.json"))?;
            info!("Loaded {} proxies from stored pool", combined.len());

            let verified = verifier::verify_pool(combined).await;

            // If unhealthy (too few alive), scrape fresh and merge
            let needs_scrape = !verifier::is_pool_healthy(&verified, 6 * 3600) || cli.force_scrape;
            let refreshed = if needs_scrape {
                warn!("Pool is stale or too small — scraping fresh proxies to fill delta...");
                let raw = run_scraper(&workspace, cli.limit, &cli.protocol)?;
                let mut merged = verified;
                merged.extend(raw);
                merged
            } else {
                info!("Pool is healthy — skipping scrape");
                verified
            };

            let (dns, non_dns, combined) = run_polish(&workspace, refreshed)?;
            let decision = rotator::build_chain_decision(&cli.mode, &dns, &non_dns, &combined);
            if let Some(d) = decision {
                print_decision(&d);
            } else {
                error!("Failed to build chain after refresh");
            }
            print_summary(combined.len(), dns.len(), non_dns.len());
        }
        "full" => {
            let raw = run_scraper(&workspace, cli.limit, &cli.protocol)?;
            let (dns, non_dns, combined) = run_polish(&workspace, raw)?;
            let decision = rotator::build_chain_decision(&cli.mode, &dns, &non_dns, &combined);

            if let Some(d) = decision {
                print_decision(&d);
            } else {
                error!("Failed to build chain");
            }

            // Print summary
            print_summary(combined.len(), dns.len(), non_dns.len());
        }
        _ => {
            error!("Unknown step: {}", cli.step);
        }
    }

    Ok(())
}

fn run_scraper(workspace: &PathBuf, limit: usize, protocol: &str) -> Result<Vec<Proxy>> {
    // Note: This Rust standalone binary calls the Go scraper as a subprocess.
    // The primary Go orchestrator (orchestrator.go + scraper.go) has the scraper
    // compiled in and does not require a separate binary.
    info!("Starting Go scraper...");
    let scraper_path = workspace.join("go_scraper");

    // Check if scraper exists
    if !scraper_path.exists() {
        anyhow::bail!("go_scraper binary not found at {}. Build with: go build -o go_scraper scraper.go", scraper_path.display());
    }

    let output = Command::new(&scraper_path)
        .arg("--limit")
        .arg(limit.to_string())
        .arg("--protocol")
        .arg(protocol)
        .output()
        .context("Failed to execute go_scraper")?;

    if !output.status.success() {
        error!(
            "Go scraper stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        anyhow::bail!(
            "Go scraper failed with exit code: {:?}",
            output.status.code()
        );
    }

    let raw_json = String::from_utf8(output.stdout)?;

    // Check if empty
    if raw_json.trim().is_empty() {
        info!("Go scraper returned empty output");
        return Ok(Vec::new());
    }

    // Save raw
    fs::write(workspace.join("raw_proxies.json"), &raw_json)?;

    // Parse
    let proxies: Vec<Proxy> =
        serde_json::from_str(&raw_json).context("Failed to parse go_scraper output")?;
    info!("Scraped {} proxies", proxies.len());
    Ok(proxies)
}

fn run_polish(
    workspace: &PathBuf,
    proxies: Vec<Proxy>,
) -> Result<(Vec<Proxy>, Vec<Proxy>, Vec<Proxy>)> {
    info!("Polishing {} proxies...", proxies.len());
    let unique = polish::deduplicate_proxies(proxies);
    let scored = polish::calculate_scores(unique);
    let (dns, non_dns) = polish::split_proxy_pools(scored.clone());

    // Save pools
    fs::write(
        workspace.join("proxies_dns.json"),
        serde_json::to_string_pretty(&dns)?,
    )?;
    fs::write(
        workspace.join("proxies_non_dns.json"),
        serde_json::to_string_pretty(&non_dns)?,
    )?;
    fs::write(
        workspace.join("proxies_combined.json"),
        serde_json::to_string_pretty(&scored)?,
    )?;

    Ok((dns, non_dns, scored))
}

fn load_proxies(path: &PathBuf) -> Result<Vec<Proxy>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(path)?;
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }
    Ok(serde_json::from_str(&content)?)
}

fn load_pools(workspace: &PathBuf) -> Result<(Vec<Proxy>, Vec<Proxy>, Vec<Proxy>)> {
    let dns = load_proxies(&workspace.join("proxies_dns.json"))?;
    let non_dns = load_proxies(&workspace.join("proxies_non_dns.json"))?;
    let combined = load_proxies(&workspace.join("proxies_combined.json"))?;
    Ok((dns, non_dns, combined))
}

fn print_decision(d: &RotationDecision) {
    println!("{}", serde_json::to_string_pretty(d).unwrap());
}

fn print_stats(workspace: &PathBuf) -> Result<()> {
    let (dns, non_dns, combined) = load_pools(workspace)?;
    println!("\n=== Spectre Network Stats ===");
    println!("Total proxies (Combined): {}", combined.len());
    println!("DNS-Capable: {}", dns.len());
    println!("Non-DNS: {}", non_dns.len());

    if !combined.is_empty() {
        let avg_latency: f64 =
            combined.iter().map(|p| p.latency).sum::<f64>() / combined.len() as f64;
        let avg_score: f64 = combined.iter().map(|p| p.score).sum::<f64>() / combined.len() as f64;
        println!("Average Latency: {:.3}s", avg_latency);
        println!("Average Score: {:.3}", avg_score);
    }
    Ok(())
}

fn print_summary(total: usize, dns: usize, non_dns: usize) {
    println!("\n=== Spectre Polish Summary ===");
    println!("Total proxies: {}", total);
    println!("DNS-capable: {}", dns);
    println!("Non-DNS: {}", non_dns);
}
