use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod http;
mod tcp;
mod uds;
mod config;
mod runner;
mod report;
mod error;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(short, long, help = "Number of concurrent connections")]
    concurrency: Option<usize>,

    #[arg(short, long, help = "Total number of requests")]
    requests: Option<usize>,

    #[arg(short, long, help = "Duration of the test in seconds")]
    duration: Option<u64>,

    #[arg(short, long, help = "Timeout for each request in milliseconds")]
    timeout: Option<u64>,
    
    #[arg(long, help = "Keep connections alive")]
    keep_alive: bool,
    
    #[arg(short, long, help = "Path to config file")]
    config: Option<PathBuf>,
    
    #[arg(long, help = "Output format (text, json)")]
    output: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "Benchmark HTTP server")]
    Http {
        #[arg(help = "URL to benchmark")]
        url: String,
        
        #[arg(short, long, help = "HTTP method")]
        method: Option<String>,
        
        #[arg(short, long, help = "Headers in format 'key:value'")]
        headers: Option<Vec<String>>,
        
        #[arg(short, long, help = "Body content for POST/PUT")]
        body: Option<String>,
        
        #[arg(long, help = "Path to body file")]
        body_file: Option<PathBuf>,
    },
    
    #[command(about = "Benchmark TCP server")]
    Tcp {
        #[arg(help = "Host:port to benchmark")]
        address: String,
        
        #[arg(short, long, help = "Data to send")]
        data: Option<String>,
        
        #[arg(long, help = "Path to data file")]
        data_file: Option<PathBuf>,
        
        #[arg(short, long, help = "Expected response pattern (regex)")]
        expect: Option<String>,
    },
    
    #[command(about = "Benchmark Unix Domain Socket server")]
    Uds {
        #[arg(help = "Socket path")]
        path: PathBuf,
        
        #[arg(short, long, help = "Data to send")]
        data: Option<String>,
        
        #[arg(long, help = "Path to data file")]
        data_file: Option<PathBuf>,
        
        #[arg(short, long, help = "Expected response pattern (regex)")]
        expect: Option<String>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Http { url, method, headers, body, body_file } => {
            let config = config::HttpConfig::new(
                url, 
                method, 
                headers, 
                body, 
                body_file,
                cli.concurrency,
                cli.requests,
                cli.duration,
                cli.timeout,
                cli.keep_alive,
            );
            
            let runner = runner::HttpRunner::new(config);
            let report = runner.run().await?;
            report::print_report(&report, cli.output.as_deref());
        },
        Commands::Tcp { address, data, data_file, expect } => {
            let config = config::TcpConfig::new(
                address,
                data,
                data_file,
                expect,
                cli.concurrency,
                cli.requests,
                cli.duration,
                cli.timeout,
                cli.keep_alive,
            );
            
            let runner = runner::TcpRunner::new(config);
            let report = runner.run().await?;
            report::print_report(&report, cli.output.as_deref());
        },
        Commands::Uds { path, data, data_file, expect } => {
            let config = config::UdsConfig::new(
                path,
                data,
                data_file,
                expect,
                cli.concurrency,
                cli.requests,
                cli.duration,
                cli.timeout,
                cli.keep_alive,
            );
            
            let runner = runner::UdsRunner::new(config);
            let report = runner.run().await?;
            report::print_report(&report, cli.output.as_deref());
        }
    }
    
    Ok(())
}