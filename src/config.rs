use std::path::PathBuf;
use std::time::Duration;
use std::fs;
use std::str::FromStr;
use crate::error::BenchmarkError;

const DEFAULT_CONCURRENCY: usize = 1;
const DEFAULT_REQUESTS: usize = 100;
const DEFAULT_DURATION: u64 = 10; // seconds
const DEFAULT_TIMEOUT: u64 = 30000; // milliseconds
const DEFAULT_METHOD: &str = "GET";

pub trait BenchmarkConfig {
    fn get_concurrency(&self) -> usize;
    fn get_requests(&self) -> usize;
    fn get_duration(&self) -> Duration;
    fn get_timeout(&self) -> Duration;
    fn is_keep_alive(&self) -> bool;
}

pub struct HttpConfig {
    pub url: String,
    pub method: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
    pub concurrency: usize,
    pub requests: usize,
    pub duration: Duration,
    pub timeout: Duration,
    pub keep_alive: bool,
}

impl HttpConfig {
    pub fn new(
        url: String,
        method: Option<String>,
        headers: Option<Vec<String>>,
        body: Option<String>,
        body_file: Option<PathBuf>,
        concurrency: Option<usize>,
        requests: Option<usize>,
        duration: Option<u64>,
        timeout: Option<u64>,
        keep_alive: bool,
    ) -> Self {
        // Process headers
        let headers = match headers {
            Some(h) => h.iter()
                .filter_map(|h| {
                    let parts: Vec<&str> = h.splitn(2, ':').collect();
                    if parts.len() == 2 {
                        Some((parts[0].trim().to_string(), parts[1].trim().to_string()))
                    } else {
                        None
                    }
                })
                .collect(),
            None => Vec::new(),
        };
        
        // Process body
        let body = if let Some(b) = body {
            Some(b.into_bytes())
        } else if let Some(path) = body_file {
            match fs::read(&path) {
                Ok(content) => Some(content),
                Err(_) => None,
            }
        } else {
            None
        };
        
        HttpConfig {
            url,
            method: method.unwrap_or_else(|| DEFAULT_METHOD.to_string()),
            headers,
            body,
            concurrency: concurrency.unwrap_or(DEFAULT_CONCURRENCY),
            requests: requests.unwrap_or(DEFAULT_REQUESTS),
            duration: Duration::from_secs(duration.unwrap_or(DEFAULT_DURATION)),
            timeout: Duration::from_millis(timeout.unwrap_or(DEFAULT_TIMEOUT)),
            keep_alive,
        }
    }
}

impl BenchmarkConfig for HttpConfig {
    fn get_concurrency(&self) -> usize {
        self.concurrency
    }
    
    fn get_requests(&self) -> usize {
        self.requests
    }
    
    fn get_duration(&self) -> Duration {
        self.duration
    }
    
    fn get_timeout(&self) -> Duration {
        self.timeout
    }
    
    fn is_keep_alive(&self) -> bool {
        self.keep_alive
    }
}

pub struct TcpConfig {
    pub address: String,
    pub data: Option<Vec<u8>>,
    pub expect: Option<String>,
    pub concurrency: usize,
    pub requests: usize,
    pub duration: Duration,
    pub timeout: Duration,
    pub keep_alive: bool,
}

impl TcpConfig {
    pub fn new(
        address: String,
        data: Option<String>,
        data_file: Option<PathBuf>,
        expect: Option<String>,
        concurrency: Option<usize>,
        requests: Option<usize>,
        duration: Option<u64>,
        timeout: Option<u64>,
        keep_alive: bool,
    ) -> Self {
        // Process data
        let data = if let Some(d) = data {
            Some(d.into_bytes())
        } else if let Some(path) = data_file {
            match fs::read(&path) {
                Ok(content) => Some(content),
                Err(_) => None,
            }
        } else {
            None
        };
        
        TcpConfig {
            address,
            data,
            expect,
            concurrency: concurrency.unwrap_or(DEFAULT_CONCURRENCY),
            requests: requests.unwrap_or(DEFAULT_REQUESTS),
            duration: Duration::from_secs(duration.unwrap_or(DEFAULT_DURATION)),
            timeout: Duration::from_millis(timeout.unwrap_or(DEFAULT_TIMEOUT)),
            keep_alive,
        }
    }
}

impl BenchmarkConfig for TcpConfig {
    fn get_concurrency(&self) -> usize {
        self.concurrency
    }
    
    fn get_requests(&self) -> usize {
        self.requests
    }
    
    fn get_duration(&self) -> Duration {
        self.duration
    }
    
    fn get_timeout(&self) -> Duration {
        self.timeout
    }
    
    fn is_keep_alive(&self) -> bool {
        self.keep_alive
    }
}

pub struct UdsConfig {
    pub path: PathBuf,
    pub data: Option<Vec<u8>>,
    pub expect: Option<String>,
    pub concurrency: usize,
    pub requests: usize,
    pub duration: Duration,
    pub timeout: Duration,
    pub keep_alive: bool,
}

impl UdsConfig {
    pub fn new(
        path: PathBuf,
        data: Option<String>,
        data_file: Option<PathBuf>,
        expect: Option<String>,
        concurrency: Option<usize>,
        requests: Option<usize>,
        duration: Option<u64>,
        timeout: Option<u64>,
        keep_alive: bool,
    ) -> Self {
        // Process data
        let data = if let Some(d) = data {
            Some(d.into_bytes())
        } else if let Some(path) = data_file {
            match fs::read(&path) {
                Ok(content) => Some(content),
                Err(_) => None,
            }
        } else {
            None
        };
        
        UdsConfig {
            path,
            data,
            expect,
            concurrency: concurrency.unwrap_or(DEFAULT_CONCURRENCY),
            requests: requests.unwrap_or(DEFAULT_REQUESTS),
            duration: Duration::from_secs(duration.unwrap_or(DEFAULT_DURATION)),
            timeout: Duration::from_millis(timeout.unwrap_or(DEFAULT_TIMEOUT)),
            keep_alive,
        }
    }
}

impl BenchmarkConfig for UdsConfig {
    fn get_concurrency(&self) -> usize {
        self.concurrency
    }
    
    fn get_requests(&self) -> usize {
        self.requests
    }
    
    fn get_duration(&self) -> Duration {
        self.duration
    }
    
    fn get_timeout(&self) -> Duration {
        self.timeout
    }
    
    fn is_keep_alive(&self) -> bool {
        self.keep_alive
    }
}