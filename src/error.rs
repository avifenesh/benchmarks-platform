use thiserror::Error;
use std::io;
use std::time::Duration;

#[derive(Debug, Error)]
pub enum BenchmarkError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    
    #[error("HTTP error: {0}")]
    Http(#[from] hyper::Error),
    
    #[error("Connection refused")]
    ConnectionRefused,
    
    #[error("Connection timed out after {0:?}")]
    ConnectionTimeout(Duration),
    
    #[error("Request timed out after {0:?}")]
    RequestTimeout(Duration),
    
    #[error("Config error: {0}")]
    Config(String),
    
    #[error("Response validation failed: {0}")]
    ResponseValidation(String),
    
    #[error("Parse error: {0}")]
    Parse(String),
    
    #[error("Unexpected error: {0}")]
    Other(String),
}

impl From<String> for BenchmarkError {
    fn from(s: String) -> Self {
        BenchmarkError::Other(s)
    }
}

impl From<&str> for BenchmarkError {
    fn from(s: &str) -> Self {
        BenchmarkError::Other(s.to_string())
    }
}