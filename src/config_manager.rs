use serde::{Serialize, Deserialize};
use std::{collections::HashMap, fs, path::{Path, PathBuf}};
use anyhow::{Result, Context};

#[derive(Serialize, Deserialize, Clone)]
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

#[derive(Serialize, Deserialize, Clone)]
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

#[derive(Serialize, Deserialize, Clone)]
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

#[derive(Serialize, Deserialize, Clone)]
pub enum BenchmarkConfigType {
    Http(HttpConfigSave),
    Tcp(TcpConfigSave),
    Uds(UdsConfigSave),
}

#[derive(Serialize, Deserialize, Default)]
pub struct ConfigStore {
    configs: HashMap<String, BenchmarkConfigType>,
}

impl ConfigStore {
    pub fn new() -> Self {
        ConfigStore { configs: HashMap::new() }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let data = fs::read_to_string(path).with_context(|| format!("Reading {:?}", path))?;
        let store = serde_json::from_str(&data).with_context(|| "Parsing config JSON")?;
        Ok(store)
    }

    pub fn save(&self, path: PathBuf) -> Result<()> {
        let json = serde_json::to_string_pretty(&self).with_context(|| "Serializing configs")?;
        fs::write(path, json).with_context(|| "Writing config file")?;
        Ok(())
    }

    pub fn add(&mut self, name: &str, cfg: BenchmarkConfigType) {
        self.configs.insert(name.to_string(), cfg);
    }

    pub fn list(&self) -> Vec<String> {
        let mut keys: Vec<_> = self.configs.keys().cloned().collect();
        keys.sort();
        keys
    }

    pub fn get(&self, name: &str) -> Option<BenchmarkConfigType> {
        self.configs.get(name).cloned()
    }

    pub fn remove(&mut self, name: &str) -> Option<BenchmarkConfigType> {
        self.configs.remove(name)
    }
}

pub fn get_default_config_path() -> Result<PathBuf> {
    let dir = dirs::config_dir().context("Couldn't find config dir")?.join("thrustbench");
    fs::create_dir_all(&dir).with_context(|| format!("Make dir {:?}", &dir))?;
    Ok(dir.join("configs.json"))
}