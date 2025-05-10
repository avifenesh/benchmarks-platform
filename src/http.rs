use std::time::{Duration, Instant};
use hyper::Uri;
use hyper::client::conn::http1::Builder;
use hyper::client::conn::http2;
use hyper_util::rt::TokioExecutor;
use hyper::Request;
use http_body_util::{BodyExt, Full};
use hyper::{Method, StatusCode};
use tokio::net::TcpStream;
use tokio::time::timeout;
use crate::error::BenchmarkError;

pub async fn send_request(
    uri: &Uri,
    method: &str,
    headers: &[(String, String)],
    body: Option<&[u8]>,
    timeout_duration: Duration,
    use_http2: bool,
) -> Result<(StatusCode, Vec<u8>, Duration), BenchmarkError> {
    let start_time = Instant::now();
    
    let host = uri.host().ok_or_else(|| BenchmarkError::Config("Missing host in URL".to_string()))?;
    let port = uri.port_u16().unwrap_or(if uri.scheme_str() == Some("https") { 443 } else { 80 });
    
    // Establish connection
    let stream = match timeout(
        timeout_duration,
        TcpStream::connect(format!("{}:{}", host, port)),
    ).await {
        Ok(Ok(stream)) => stream,
        Ok(Err(_)) => return Err(BenchmarkError::ConnectionRefused),
        Err(_) => return Err(BenchmarkError::ConnectionTimeout(timeout_duration)),
    };
    
    // Prepare request
    let method = Method::from_bytes(method.as_bytes())
        .map_err(|_| BenchmarkError::Parse(format!("Invalid HTTP method: {}", method)))?;
    
    let mut request_builder = Request::builder()
        .method(method)
        .uri(uri.clone());
    
    // Add headers
    for (name, value) in headers {
        request_builder = request_builder.header(name, value);
    }
    
    // Add body if present
    let body_data = body.unwrap_or(&[]);
    let request = request_builder
        .body(Full::new(bytes::Bytes::from(body_data.to_vec())))
        .map_err(|_| BenchmarkError::Parse("Failed to build request".to_string()))?;
    
    // Send request and get response
    let (status, body_bytes) = if use_http2 {
        // HTTP/2 connection
        let (mut sender, conn) = http2::handshake(TokioExecutor::new(), stream, Default::default()).await
            .map_err(|e| BenchmarkError::Http(e))?;
        
        // Spawn connection task
        tokio::spawn(async move {
            if let Err(e) = conn.await {
                eprintln!("HTTP/2 connection error: {}", e);
            }
        });
        
        // Send request
        let response = timeout(
            timeout_duration,
            sender.send_request(request),
        ).await
            .map_err(|_| BenchmarkError::RequestTimeout(timeout_duration))??;
        
        let status = response.status();
        
        // Get response body
        let body = timeout(
            timeout_duration,
            response.collect(),
        ).await
            .map_err(|_| BenchmarkError::RequestTimeout(timeout_duration))??;
        
        let bytes = body.to_bytes();
        (status, bytes.to_vec())
    } else {
        // HTTP/1.x connection
        let (mut sender, conn) = Builder::new()
            .handshake::<TcpStream, Full<bytes::Bytes>>(stream)
            .await
            .map_err(|e| BenchmarkError::Http(e))?;
        
        // Spawn connection task
        tokio::spawn(async move {
            if let Err(e) = conn.await {
                eprintln!("HTTP/1 connection error: {}", e);
            }
        });
        
        // Send request
        let response = timeout(
            timeout_duration,
            sender.send_request(request),
        ).await
            .map_err(|_| BenchmarkError::RequestTimeout(timeout_duration))??;
        
        let status = response.status();
        
        // Get response body
        let body = timeout(
            timeout_duration,
            response.collect(),
        ).await
            .map_err(|_| BenchmarkError::RequestTimeout(timeout_duration))??;
        
        let bytes = body.to_bytes();
        (status, bytes.to_vec())
    };
    
    let elapsed = start_time.elapsed();
    Ok((status, body_bytes, elapsed))
}