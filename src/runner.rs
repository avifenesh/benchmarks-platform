use std::time::{Duration, Instant};
use tokio::time::sleep;
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use hyper::Uri;
use hyper::StatusCode;
use futures::future::{join_all, BoxFuture};
use indicatif::{ProgressBar, ProgressStyle};

use crate::config::{BenchmarkConfig, HttpConfig, TcpConfig, UdsConfig};
use crate::report::BenchmarkReport;
use crate::error::BenchmarkError;
use crate::http;
use crate::tcp;
use crate::uds;

const BUFFER_SIZE: usize = 8192;

pub struct HttpRunner {
    config: HttpConfig,
}

impl HttpRunner {
    pub fn new(config: HttpConfig) -> Self {
        HttpRunner { config }
    }
    
    pub async fn run(&self) -> Result<BenchmarkReport, BenchmarkError> {
        let uri: Uri = self.config.url.parse()
            .map_err(|_| BenchmarkError::Config(format!("Invalid URL: {}", self.config.url)))?;
        
        println!("Starting HTTP benchmark for {} with {} connections...", self.config.url, self.config.concurrency);
        
        // Create progress bar
        let progress = if self.config.requests > 0 {
            let bar = ProgressBar::new(self.config.requests as u64);
            bar.set_style(
                ProgressStyle::default_bar()
                    .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {percent}% ({eta})")
                    .unwrap()
                    .progress_chars("##-")
            );
            Some(bar)
        } else {
            None
        };
        
        let concurrency = self.config.concurrency;
        let requests_per_worker = if self.config.requests > 0 {
            (self.config.requests + concurrency - 1) / concurrency // ceiling division
        } else {
            usize::MAX // run forever until duration is reached
        };
        
        let start_time = Instant::now();
        let stop_time = start_time + self.config.duration;
        
        // Shared counters for all workers
        let completed_requests = Arc::new(AtomicUsize::new(0));
        let successful_requests = Arc::new(AtomicUsize::new(0));
        let bytes_sent = Arc::new(AtomicUsize::new(0));
        let bytes_received = Arc::new(AtomicUsize::new(0));
        
        // Channel for response times
        let (tx, mut rx) = mpsc::channel::<Duration>(10000);
        
        // Spawn worker tasks
        let mut set = JoinSet::new();
        
        for _ in 0..concurrency {
            let uri = uri.clone();
            let method = self.config.method.clone();
            let headers = self.config.headers.clone();
            let body = self.config.body.clone();
            let timeout_duration = self.config.timeout;
            let keep_alive = self.config.is_keep_alive();
            let completed_clone = completed_requests.clone();
            let successful_clone = successful_requests.clone();
            let bytes_sent_clone = bytes_sent.clone();
            let bytes_received_clone = bytes_received.clone();
            let tx_clone = tx.clone();
            let progress_clone = progress.clone();
            
            set.spawn(async move {
                let mut conn_reuse = None;
                
                for _ in 0..requests_per_worker {
                    if Instant::now() >= stop_time {
                        break;
                    }
                    
                    // TODO: Handle connection reuse when keep_alive is true
                    
                    // Send request
                    match http::send_request(
                        &uri,
                        &method,
                        &headers,
                        body.as_deref(),
                        timeout_duration,
                        false, // use HTTP/1.1
                    ).await {
                        Ok((status, body, elapsed)) => {
                            successful_clone.fetch_add(1, Ordering::Relaxed);
                            bytes_received_clone.fetch_add(body.len(), Ordering::Relaxed);
                            
                            if let Some(body_size) = body.len().checked_add(
                                headers.iter().fold(0, |acc, (k, v)| acc + k.len() + v.len())
                            ) {
                                bytes_sent_clone.fetch_add(body_size, Ordering::Relaxed);
                            }
                            
                            let _ = tx_clone.send(elapsed).await;
                        },
                        Err(_) => {
                            // Error handling is already done in the http module
                        }
                    }
                    
                    completed_clone.fetch_add(1, Ordering::Relaxed);
                    
                    if let Some(ref bar) = progress_clone {
                        bar.inc(1);
                    }
                }
            });
        }
        
        // Drop the original sender so the channel can close when all workers are done
        drop(tx);
        
        // Wait for all workers to complete or timeout
        while (Instant::now() < stop_time) && (set.len() > 0) {
            tokio::select! {
                _ = sleep(Duration::from_millis(100)) => {
                    // Just a timeout to check if we've reached the stop time
                }
                _ = set.join_next() => {
                    // A worker has completed
                }
            }
        }
        
        // Cancel any remaining tasks
        set.abort_all();
        
        // Collect all response times
        let mut response_times = Vec::new();
        while let Some(time) = rx.recv().await {
            response_times.push(time);
        }
        
        if let Some(bar) = progress {
            bar.finish_and_clear();
        }
        
        // Sort response times for percentiles
        response_times.sort();
        
        // Calculate statistics
        let total_time = start_time.elapsed();
        let total_requests = completed_requests.load(Ordering::Relaxed);
        let successful = successful_requests.load(Ordering::Relaxed);
        let failed = total_requests.saturating_sub(successful);
        
        let avg_time = if response_times.is_empty() {
            Duration::from_secs(0)
        } else {
            response_times.iter().fold(Duration::from_secs(0), |acc, &x| acc + x) 
                / response_times.len() as u32
        };
        
        let min_time = response_times.first().cloned().unwrap_or_else(|| Duration::from_secs(0));
        let max_time = response_times.last().cloned().unwrap_or_else(|| Duration::from_secs(0));
        
        let p50 = percentile(&response_times, 0.5);
        let p90 = percentile(&response_times, 0.9);
        let p95 = percentile(&response_times, 0.95);
        let p99 = percentile(&response_times, 0.99);
        
        let requests_per_second = if total_time.as_secs_f64() > 0.0 {
            total_requests as f64 / total_time.as_secs_f64()
        } else {
            0.0
        };
        
        Ok(BenchmarkReport {
            target: self.config.url.clone(),
            protocol: "HTTP".to_string(),
            concurrency: self.config.concurrency,
            total_requests,
            successful_requests: successful,
            failed_requests: failed,
            total_time,
            requests_per_second,
            avg_response_time: avg_time,
            min_response_time: min_time,
            max_response_time: max_time,
            p50_response_time: p50,
            p90_response_time: p90,
            p95_response_time: p95,
            p99_response_time: p99,
            bytes_sent: bytes_sent.load(Ordering::Relaxed) as u64,
            bytes_received: bytes_received.load(Ordering::Relaxed) as u64,
        })
    }
}

pub struct TcpRunner {
    config: TcpConfig,
}

impl TcpRunner {
    pub fn new(config: TcpConfig) -> Self {
        TcpRunner { config }
    }
    
    pub async fn run(&self) -> Result<BenchmarkReport, BenchmarkError> {
        println!("Starting TCP benchmark for {} with {} connections...", self.config.address, self.config.concurrency);
        
        // Create progress bar
        let progress = if self.config.requests > 0 {
            let bar = ProgressBar::new(self.config.requests as u64);
            bar.set_style(
                ProgressStyle::default_bar()
                    .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {percent}% ({eta})")
                    .unwrap()
                    .progress_chars("##-")
            );
            Some(bar)
        } else {
            None
        };
        
        let concurrency = self.config.concurrency;
        let requests_per_worker = if self.config.requests > 0 {
            (self.config.requests + concurrency - 1) / concurrency // ceiling division
        } else {
            usize::MAX // run forever until duration is reached
        };
        
        let start_time = Instant::now();
        let stop_time = start_time + self.config.duration;
        
        // Shared counters for all workers
        let completed_requests = Arc::new(AtomicUsize::new(0));
        let successful_requests = Arc::new(AtomicUsize::new(0));
        let bytes_sent = Arc::new(AtomicUsize::new(0));
        let bytes_received = Arc::new(AtomicUsize::new(0));
        
        // Channel for response times
        let (tx, mut rx) = mpsc::channel::<Duration>(10000);
        
        // Spawn worker tasks
        let mut set = JoinSet::new();
        
        for _ in 0..concurrency {
            let address = self.config.address.clone();
            let data = self.config.data.clone();
            let expect = self.config.expect.clone();
            let timeout_duration = self.config.timeout;
            let completed_clone = completed_requests.clone();
            let successful_clone = successful_requests.clone();
            let bytes_sent_clone = bytes_sent.clone();
            let bytes_received_clone = bytes_received.clone();
            let tx_clone = tx.clone();
            let progress_clone = progress.clone();
            
            set.spawn(async move {
                for _ in 0..requests_per_worker {
                    if Instant::now() >= stop_time {
                        break;
                    }
                    
                    // Send TCP request
                    match tcp::send_tcp(
                        &address,
                        data.as_deref(),
                        expect.as_deref(),
                        timeout_duration,
                        BUFFER_SIZE,
                    ).await {
                        Ok((response, elapsed)) => {
                            successful_clone.fetch_add(1, Ordering::Relaxed);
                            bytes_received_clone.fetch_add(response.len(), Ordering::Relaxed);
                            
                            if let Some(ref d) = data {
                                bytes_sent_clone.fetch_add(d.len(), Ordering::Relaxed);
                            }
                            
                            let _ = tx_clone.send(elapsed).await;
                        },
                        Err(_) => {
                            // Error handling is already done in the tcp module
                        }
                    }
                    
                    completed_clone.fetch_add(1, Ordering::Relaxed);
                    
                    if let Some(ref bar) = progress_clone {
                        bar.inc(1);
                    }
                }
            });
        }
        
        // Drop the original sender so the channel can close when all workers are done
        drop(tx);
        
        // Wait for all workers to complete or timeout
        while (Instant::now() < stop_time) && (set.len() > 0) {
            tokio::select! {
                _ = sleep(Duration::from_millis(100)) => {
                    // Just a timeout to check if we've reached the stop time
                }
                _ = set.join_next() => {
                    // A worker has completed
                }
            }
        }
        
        // Cancel any remaining tasks
        set.abort_all();
        
        // Collect all response times
        let mut response_times = Vec::new();
        while let Some(time) = rx.recv().await {
            response_times.push(time);
        }
        
        if let Some(bar) = progress {
            bar.finish_and_clear();
        }
        
        // Sort response times for percentiles
        response_times.sort();
        
        // Calculate statistics
        let total_time = start_time.elapsed();
        let total_requests = completed_requests.load(Ordering::Relaxed);
        let successful = successful_requests.load(Ordering::Relaxed);
        let failed = total_requests.saturating_sub(successful);
        
        let avg_time = if response_times.is_empty() {
            Duration::from_secs(0)
        } else {
            response_times.iter().fold(Duration::from_secs(0), |acc, &x| acc + x) 
                / response_times.len() as u32
        };
        
        let min_time = response_times.first().cloned().unwrap_or_else(|| Duration::from_secs(0));
        let max_time = response_times.last().cloned().unwrap_or_else(|| Duration::from_secs(0));
        
        let p50 = percentile(&response_times, 0.5);
        let p90 = percentile(&response_times, 0.9);
        let p95 = percentile(&response_times, 0.95);
        let p99 = percentile(&response_times, 0.99);
        
        let requests_per_second = if total_time.as_secs_f64() > 0.0 {
            total_requests as f64 / total_time.as_secs_f64()
        } else {
            0.0
        };
        
        Ok(BenchmarkReport {
            target: self.config.address.clone(),
            protocol: "TCP".to_string(),
            concurrency: self.config.concurrency,
            total_requests,
            successful_requests: successful,
            failed_requests: failed,
            total_time,
            requests_per_second,
            avg_response_time: avg_time,
            min_response_time: min_time,
            max_response_time: max_time,
            p50_response_time: p50,
            p90_response_time: p90,
            p95_response_time: p95,
            p99_response_time: p99,
            bytes_sent: bytes_sent.load(Ordering::Relaxed) as u64,
            bytes_received: bytes_received.load(Ordering::Relaxed) as u64,
        })
    }
}

pub struct UdsRunner {
    config: UdsConfig,
}

impl UdsRunner {
    pub fn new(config: UdsConfig) -> Self {
        UdsRunner { config }
    }
    
    pub async fn run(&self) -> Result<BenchmarkReport, BenchmarkError> {
        println!("Starting Unix Domain Socket benchmark for {:?} with {} connections...", 
                 self.config.path, self.config.concurrency);
        
        // Create progress bar
        let progress = if self.config.requests > 0 {
            let bar = ProgressBar::new(self.config.requests as u64);
            bar.set_style(
                ProgressStyle::default_bar()
                    .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {percent}% ({eta})")
                    .unwrap()
                    .progress_chars("##-")
            );
            Some(bar)
        } else {
            None
        };
        
        let concurrency = self.config.concurrency;
        let requests_per_worker = if self.config.requests > 0 {
            (self.config.requests + concurrency - 1) / concurrency // ceiling division
        } else {
            usize::MAX // run forever until duration is reached
        };
        
        let start_time = Instant::now();
        let stop_time = start_time + self.config.duration;
        
        // Shared counters for all workers
        let completed_requests = Arc::new(AtomicUsize::new(0));
        let successful_requests = Arc::new(AtomicUsize::new(0));
        let bytes_sent = Arc::new(AtomicUsize::new(0));
        let bytes_received = Arc::new(AtomicUsize::new(0));
        
        // Channel for response times
        let (tx, mut rx) = mpsc::channel::<Duration>(10000);
        
        // Spawn worker tasks
        let mut set = JoinSet::new();
        
        for _ in 0..concurrency {
            let path = self.config.path.clone();
            let data = self.config.data.clone();
            let expect = self.config.expect.clone();
            let timeout_duration = self.config.timeout;
            let completed_clone = completed_requests.clone();
            let successful_clone = successful_requests.clone();
            let bytes_sent_clone = bytes_sent.clone();
            let bytes_received_clone = bytes_received.clone();
            let tx_clone = tx.clone();
            let progress_clone = progress.clone();
            
            set.spawn(async move {
                for _ in 0..requests_per_worker {
                    if Instant::now() >= stop_time {
                        break;
                    }
                    
                    // Send UDS request
                    match uds::send_uds(
                        &path,
                        data.as_deref(),
                        expect.as_deref(),
                        timeout_duration,
                        BUFFER_SIZE,
                    ).await {
                        Ok((response, elapsed)) => {
                            successful_clone.fetch_add(1, Ordering::Relaxed);
                            bytes_received_clone.fetch_add(response.len(), Ordering::Relaxed);
                            
                            if let Some(ref d) = data {
                                bytes_sent_clone.fetch_add(d.len(), Ordering::Relaxed);
                            }
                            
                            let _ = tx_clone.send(elapsed).await;
                        },
                        Err(_) => {
                            // Error handling is already done in the uds module
                        }
                    }
                    
                    completed_clone.fetch_add(1, Ordering::Relaxed);
                    
                    if let Some(ref bar) = progress_clone {
                        bar.inc(1);
                    }
                }
            });
        }
        
        // Drop the original sender so the channel can close when all workers are done
        drop(tx);
        
        // Wait for all workers to complete or timeout
        while (Instant::now() < stop_time) && (set.len() > 0) {
            tokio::select! {
                _ = sleep(Duration::from_millis(100)) => {
                    // Just a timeout to check if we've reached the stop time
                }
                _ = set.join_next() => {
                    // A worker has completed
                }
            }
        }
        
        // Cancel any remaining tasks
        set.abort_all();
        
        // Collect all response times
        let mut response_times = Vec::new();
        while let Some(time) = rx.recv().await {
            response_times.push(time);
        }
        
        if let Some(bar) = progress {
            bar.finish_and_clear();
        }
        
        // Sort response times for percentiles
        response_times.sort();
        
        // Calculate statistics
        let total_time = start_time.elapsed();
        let total_requests = completed_requests.load(Ordering::Relaxed);
        let successful = successful_requests.load(Ordering::Relaxed);
        let failed = total_requests.saturating_sub(successful);
        
        let avg_time = if response_times.is_empty() {
            Duration::from_secs(0)
        } else {
            response_times.iter().fold(Duration::from_secs(0), |acc, &x| acc + x) 
                / response_times.len() as u32
        };
        
        let min_time = response_times.first().cloned().unwrap_or_else(|| Duration::from_secs(0));
        let max_time = response_times.last().cloned().unwrap_or_else(|| Duration::from_secs(0));
        
        let p50 = percentile(&response_times, 0.5);
        let p90 = percentile(&response_times, 0.9);
        let p95 = percentile(&response_times, 0.95);
        let p99 = percentile(&response_times, 0.99);
        
        let requests_per_second = if total_time.as_secs_f64() > 0.0 {
            total_requests as f64 / total_time.as_secs_f64()
        } else {
            0.0
        };
        
        Ok(BenchmarkReport {
            target: self.config.path.to_string_lossy().to_string(),
            protocol: "Unix Domain Socket".to_string(),
            concurrency: self.config.concurrency,
            total_requests,
            successful_requests: successful,
            failed_requests: failed,
            total_time,
            requests_per_second,
            avg_response_time: avg_time,
            min_response_time: min_time,
            max_response_time: max_time,
            p50_response_time: p50,
            p90_response_time: p90,
            p95_response_time: p95,
            p99_response_time: p99,
            bytes_sent: bytes_sent.load(Ordering::Relaxed) as u64,
            bytes_received: bytes_received.load(Ordering::Relaxed) as u64,
        })
    }
}

fn percentile(durations: &[Duration], percentile: f64) -> Duration {
    if durations.is_empty() {
        return Duration::from_secs(0);
    }
    
    let index = ((durations.len() as f64) * percentile).floor() as usize;
    let index = index.min(durations.len() - 1);
    durations[index]
}