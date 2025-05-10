use std::time::Duration;
use serde::{Serialize, Deserialize};
use colored::*;
use humantime::format_duration;
use serde_json;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkReport {
    pub target: String,
    pub protocol: String,
    pub concurrency: usize,
    pub total_requests: usize,
    pub successful_requests: usize,
    pub failed_requests: usize,
    pub total_time: Duration,
    pub requests_per_second: f64,
    pub avg_response_time: Duration,
    pub min_response_time: Duration,
    pub max_response_time: Duration,
    pub p50_response_time: Duration,
    pub p90_response_time: Duration,
    pub p95_response_time: Duration,
    pub p99_response_time: Duration,
    pub bytes_sent: u64,
    pub bytes_received: u64,
}

pub fn print_report(report: &BenchmarkReport, format: Option<&str>) {
    match format {
        Some("json") => print_json_report(report),
        _ => print_text_report(report),
    }
}

fn print_text_report(report: &BenchmarkReport) {
    println!();
    println!("{}", "=".repeat(80).bright_blue());
    println!("{}", "BENCHMARK REPORT".bright_blue());
    println!("{}", "=".repeat(80).bright_blue());
    
    println!("{} {}", "Target:".bold(), report.target);
    println!("{} {}", "Protocol:".bold(), report.protocol);
    println!("{} {}", "Concurrency:".bold(), report.concurrency);
    println!();
    
    println!("{}", "Request Statistics:".bold().underline());
    println!("{} {}", "Total Requests:".bold(), report.total_requests);
    println!("{} {}", "Successful Requests:".bold(), report.successful_requests.to_string().green());
    println!("{} {}", "Failed Requests:".bold(), report.failed_requests.to_string().red());
    println!("{} {}", "Requests/sec:".bold(), format!("{:.2}", report.requests_per_second).bright_green());
    println!();
    
    println!("{}", "Timing Statistics:".bold().underline());
    println!("{} {}", "Total Time:".bold(), format_duration(report.total_time));
    println!("{} {}", "Average Response Time:".bold(), format_duration(report.avg_response_time));
    println!("{} {}", "Minimum Response Time:".bold(), format_duration(report.min_response_time));
    println!("{} {}", "Maximum Response Time:".bold(), format_duration(report.max_response_time));
    println!("{} {}", "p50 Response Time:".bold(), format_duration(report.p50_response_time));
    println!("{} {}", "p90 Response Time:".bold(), format_duration(report.p90_response_time));
    println!("{} {}", "p95 Response Time:".bold(), format_duration(report.p95_response_time));
    println!("{} {}", "p99 Response Time:".bold(), format_duration(report.p99_response_time));
    println!();
    
    println!("{}", "Transfer Statistics:".bold().underline());
    println!("{} {} bytes", "Total Data Sent:".bold(), report.bytes_sent);
    println!("{} {} bytes", "Total Data Received:".bold(), report.bytes_received);
    println!();
    
    println!("{}", "=".repeat(80).bright_blue());
}

fn print_json_report(report: &BenchmarkReport) {
    match serde_json::to_string_pretty(report) {
        Ok(json) => println!("{}", json),
        Err(_) => eprintln!("Error serializing report to JSON"),
    }
}