use std::path::PathBuf;
use clap::Parser;

#[derive(Parser)]
#[command(name = "mcp-multiplexer", version, about = "One MCP server fronting many, with 5 meta-tools")]
struct Args {
    /// Path to config file (standard mcpServers format)
    #[arg(long, default_value = "./.mcp.json")]
    config: PathBuf,
    /// Append logs to this file instead of stderr
    #[arg(long)]
    log_file: Option<PathBuf>,
    /// Debug logging
    #[arg(short, long)]
    verbose: bool,
    /// Print JSON Schema for the config file and exit
    #[arg(long)]
    dump_schema: bool,
}

fn init_logging(verbose: bool, log_file: Option<&std::path::Path>) {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(if verbose { "debug" } else { "info" }));
    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr);
    if let Some(path) = log_file {
        let file = std::fs::OpenOptions::new().create(true).append(true).open(path)
            .unwrap_or_else(|e| panic!("cannot open log file {}: {e}", path.display()));
        builder.with_writer(move || file.try_clone().expect("log file clone")).init();
    } else {
        builder.init();
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    init_logging(args.verbose, args.log_file.as_deref());
    if args.dump_schema {
        println!("{{\"placeholder\": true}}");
        return Ok(());
    }
    tracing::info!(config = %args.config.display(), "starting");
    Ok(())
}
