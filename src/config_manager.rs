use crate::config::{HttpConfig, TcpConfig, UdsConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::io::Result as IoResult;
use anyhow::{Result, Context};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum BenchmarkConfigType {
    Http(HttpConfigSave),
    Tcp(TcpConfigSave),
    Uds(UdsConfigSave),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HttpConfigSave {
    pub url: String,
    pub method: Option<String>,
    pub headers: Option<Vec<String>>,
    pub body: Option<String>,
    pub concurrency: Option<usize>,
    pub requests: Option<usize>,
    pub duration: Option<u64>,
    pub timeout: Option<u64>,
    pub keep_alive: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TcpConfigSave {
    pub address: String,
    pub data: Option<String>,
    pub expect: Option<String>,
    pub concurrency: Option<usize>,
    pub requests: Option<usize>,
    pub duration: Option<u64>,
    pub timeout: Option<u64>,
    pub keep_alive: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UdsConfigSave {
    pub path: String,
    pub data: Option<String>,
    pub expect: Option<String>,
    pub concurrency: Option<usize>,
    pub requests: Option<usize>,
    pub duration: Option<u64>,
    pub timeout: Option<u64>,
    pub keep_alive: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ConfigStore {
    configs: HashMap<String, BenchmarkConfigType>,
}

impl ConfigStore {
    /// Create a new empty config store
    pub fn new() -> Self {
        Self {
            configs: HashMap::new(),
        }
    }

    /// Load configs from a file
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let content = fs::read_to_string(path)
            .context("Failed to read config file")?;
        
        let store: ConfigStore = serde_json::from_str(&content)
            .context("Failed to parse config file")?;
        
        Ok(store)
    }

    /// Save configs to a file
    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let content = serde_json::to_string_pretty(self)
            .context("Failed to serialize config")?;
        
        fs::write(path, content)
            .context("Failed to write config file")?;
        
        Ok(())
    }

    /// Add a configuration with a name
    pub fn add(&mut self, name: &str, config: BenchmarkConfigType) {
        self.configs.insert(name.to_string(), config);
    }

    /// Get a configuration by name
    pub fn get(&self, name: &str) -> Option<&BenchmarkConfigType> {
        self.configs.get(name)
    }

    /// Remove a configuration by name
    pub fn remove(&mut self, name: &str) -> Option<BenchmarkConfigType> {
        self.configs.remove(name)
    }

    /// List all configuration names
    pub fn list(&self) -> Vec<String> {
        self.configs.keys().cloned().collect()
    }

    /// Check if the store has a configuration with the given name
    pub fn contains(&self, name: &str) -> bool {
        self.configs.contains_key(name)
    }
}

impl From<&HttpConfig> for HttpConfigSave {
    fn from(config: &HttpConfig) -> Self {
        // Convert the internal header format to the user-friendly format
        let headers = if config.headers.is_empty() {
            None
        } else {
            Some(config.headers.iter()
                .map(|(k, v)| format!("{}:{}", k, v))
                .collect())
        };

        // Convert the body bytes to a string if present
        let body = config.body.as_ref()
            .map(|b| String::from_utf8_lossy(b).to_string());

        Self {
            url: config.url.clone(),
            method: Some(config.method.clone()),
            headers,
            body,
            concurrency: Some(config.concurrency),
            requests: Some(config.requests),
            duration: Some(config.duration.as_secs()),
            timeout: Some(config.timeout.as_millis() as u64),
            keep_alive: config.keep_alive,
        }
    }
}

impl From<&TcpConfig> for TcpConfigSave {
    fn from(config: &TcpConfig) -> Self {
        // Convert the data bytes to a string if present
        let data = config.data.as_ref()
            .map(|d| String::from_utf8_lossy(d).to_string());

        Self {
            address: config.address.clone(),
            data,
            expect: config.expect.clone(),
            concurrency: Some(config.concurrency),
            requests: Some(config.requests),
            duration: Some(config.duration.as_secs()),
            timeout: Some(config.timeout.as_millis() as u64),
            keep_alive: config.keep_alive,
        }
    }
}

impl From<&UdsConfig> for UdsConfigSave {
    fn from(config: &UdsConfig) -> Self {
        // Convert the data bytes to a string if present
        let data = config.data.as_ref()
            .map(|d| String::from_utf8_lossy(d).to_string());

        Self {
            path: config.path.to_string_lossy().to_string(),
            data,
            expect: config.expect.clone(),
            concurrency: Some(config.concurrency),
            requests: Some(config.requests),
            duration: Some(config.duration.as_secs()),
            timeout: Some(config.timeout.as_millis() as u64),
            keep_alive: config.keep_alive,
        }
    }
}

impl HttpConfigSave {
    pub fn to_http_config(&self) -> HttpConfig {
        // Process headers
        let headers = match &self.headers {
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
        let body = self.body.as_ref()
            .map(|b| b.as_bytes().to_vec());
        
        HttpConfig {
            url: self.url.clone(),
            method: self.method.clone().unwrap_or_else(|| "GET".to_string()),
            headers,
            body,
            concurrency: self.concurrency.unwrap_or(1),
            requests: self.requests.unwrap_or(100),
            duration: std::time::Duration::from_secs(self.duration.unwrap_or(10)),
            timeout: std::time::Duration::from_millis(self.timeout.unwrap_or(30000)),
            keep_alive: self.keep_alive,
        }
    }
}

impl TcpConfigSave {
    pub fn to_tcp_config(&self) -> TcpConfig {
        // Process data
        let data = self.data.as_ref()
            .map(|d| d.as_bytes().to_vec());
        
        TcpConfig {
            address: self.address.clone(),
            data,
            expect: self.expect.clone(),
            concurrency: self.concurrency.unwrap_or(1),
            requests: self.requests.unwrap_or(100),
            duration: std::time::Duration::from_secs(self.duration.unwrap_or(10)),
            timeout: std::time::Duration::from_millis(self.timeout.unwrap_or(30000)),
            keep_alive: self.keep_alive,
        }
    }
}

impl UdsConfigSave {
    pub fn to_uds_config(&self) -> UdsConfig {
        // Process data
        let data = self.data.as_ref()
            .map(|d| d.as_bytes().to_vec());
        
        UdsConfig {
            path: std::path::PathBuf::from(&self.path),
            data,
            expect: self.expect.clone(),
            concurrency: self.concurrency.unwrap_or(1),
            requests: self.requests.unwrap_or(100),
            duration: std::time::Duration::from_secs(self.duration.unwrap_or(10)),
            timeout: std::time::Duration::from_millis(self.timeout.unwrap_or(30000)),
            keep_alive: self.keep_alive,
        }
    }
}

pub fn http_config_to_save(config: &HttpConfig) -> HttpConfigSave {
    config.into()
}

pub fn tcp_config_to_save(config: &TcpConfig) -> TcpConfigSave {
    config.into()
}

pub fn uds_config_to_save(config: &UdsConfig) -> UdsConfigSave {
    config.into()
}

pub fn get_config_dir() -> IoResult<std::path::PathBuf> {
    // Use the standard location for configuration files based on the platform
    let home_dir = dirs::home_dir().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Could not determine home directory",
        )
    })?;
    
    let config_dir = home_dir.join(".thrustbench");
    
    // Create the directory if it doesn't exist
    if !config_dir.exists() {
        fs::create_dir_all(&config_dir)?;
    }
    
    Ok(config_dir)
}

pub fn get_default_config_path() -> IoResult<std::path::PathBuf> {
    let config_dir = get_config_dir()?;
    Ok(config_dir.join("configs.json"))
}