use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use regex::Regex;
use crate::error::BenchmarkError;

pub async fn send_tcp(
    address: &str,
    data: Option<&[u8]>,
    expect_pattern: Option<&str>,
    timeout_duration: Duration,
    buffer_size: usize,
) -> Result<(Vec<u8>, Duration), BenchmarkError> {
    let start_time = Instant::now();
    
    // Establish connection
    let mut stream = match timeout(
        timeout_duration,
        TcpStream::connect(address),
    ).await {
        Ok(Ok(stream)) => stream,
        Ok(Err(_)) => return Err(BenchmarkError::ConnectionRefused),
        Err(_) => return Err(BenchmarkError::ConnectionTimeout(timeout_duration)),
    };
    
    // Send data if provided
    if let Some(bytes) = data {
        if !bytes.is_empty() {
            match timeout(timeout_duration, stream.write_all(bytes)).await {
                Ok(Ok(_)) => {},
                Ok(Err(e)) => return Err(BenchmarkError::Io(e)),
                Err(_) => return Err(BenchmarkError::RequestTimeout(timeout_duration)),
            }
        }
    }
    
    // Read response
    let mut response = Vec::new();
    let mut buffer = vec![0; buffer_size];
    
    // If we expect a pattern, read until we find it or timeout
    if let Some(pattern) = expect_pattern {
        let regex = Regex::new(pattern)
            .map_err(|_| BenchmarkError::Parse(format!("Invalid regex pattern: {}", pattern)))?;
        
        let deadline = Instant::now() + timeout_duration;
        let mut found = false;
        
        while Instant::now() < deadline && !found {
            match stream.read(&mut buffer).await {
                Ok(0) => break, // EOF
                Ok(n) => {
                    response.extend_from_slice(&buffer[..n]);
                    // Check if pattern is found
                    if let Ok(text) = String::from_utf8(response.clone()) {
                        if regex.is_match(&text) {
                            found = true;
                            break;
                        }
                    }
                },
                Err(e) => return Err(BenchmarkError::Io(e)),
            }
        }
        
        if !found {
            return Err(BenchmarkError::ResponseValidation(
                format!("Expected pattern '{}' not found in response", pattern)
            ));
        }
    } else {
        // Without a pattern, just read what's available within the timeout
        match timeout(timeout_duration, async {
            loop {
                match stream.read(&mut buffer).await {
                    Ok(0) => break, // EOF
                    Ok(n) => response.extend_from_slice(&buffer[..n]),
                    Err(e) => return Err(BenchmarkError::Io(e)),
                }
            }
            Ok::<(), BenchmarkError>(())
        }).await {
            Ok(Ok(_)) => {},
            Ok(Err(e)) => return Err(e),
            Err(_) => {}, // Timeout is normal when no pattern is expected
        }
    }
    
    let elapsed = start_time.elapsed();
    Ok((response, elapsed))
}