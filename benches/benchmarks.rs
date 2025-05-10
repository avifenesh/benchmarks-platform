use criterion::{black_box, criterion_group, criterion_main, Criterion};
use tokio::runtime::Runtime;
use std::time::Duration;
use vibe_coding::config::{HttpConfig, TcpConfig, UdsConfig};
use vibe_coding::runner::{HttpRunner, TcpRunner, UdsRunner};
use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::task;
use tokio::net::UnixListener;
use std::path::PathBuf;
use std::{thread, fs};

// HTTP benchmarks
fn bench_http(c: &mut Criterion) {
    // Setup a simple HTTP server
    let rt = Runtime::new().unwrap();
    
    // Start HTTP server
    let server_handle = thread::spawn(move || {
        rt.block_on(async {
            let listener = TcpListener::bind("127.0.0.1:8080").await.unwrap();
            
            println!("HTTP server listening on 127.0.0.1:8080");
            
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                
                task::spawn(async move {
                    let mut buf = [0; 1024];
                    
                    // Read request
                    let _ = stream.read(&mut buf).await.unwrap();
                    
                    // Send response
                    let response = "HTTP/1.1 200 OK\r\nContent-Length: 13\r\nContent-Type: text/plain\r\n\r\nHello, World!";
                    let _ = stream.write_all(response.as_bytes()).await;
                });
            }
        });
    });
    
    // Give server time to start
    thread::sleep(Duration::from_millis(500));
    
    // Run HTTP benchmark
    let mut group = c.benchmark_group("http");
    group.measurement_time(Duration::from_secs(5));
    group.sample_size(10);
    
    let config = HttpConfig::new(
        "http://127.0.0.1:8080".to_string(),
        Some("GET".to_string()),
        None,
        None,
        None,
        Some(10),
        Some(1000),
        Some(30),
        Some(1000),
        false,
    );
    
    group.bench_function("http_get", |b| {
        b.iter(|| {
            let runner = HttpRunner::new(config.clone());
            let rt = Runtime::new().unwrap();
            black_box(rt.block_on(async {
                runner.run().await.unwrap()
            }));
        });
    });
    
    group.finish();
}

// TCP benchmarks
fn bench_tcp(c: &mut Criterion) {
    // Setup a simple TCP echo server
    let rt = Runtime::new().unwrap();
    
    // Start TCP server
    let server_handle = thread::spawn(move || {
        rt.block_on(async {
            let listener = TcpListener::bind("127.0.0.1:8081").await.unwrap();
            
            println!("TCP server listening on 127.0.0.1:8081");
            
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                
                task::spawn(async move {
                    let mut buf = [0; 1024];
                    
                    // Read data
                    let n = stream.read(&mut buf).await.unwrap();
                    
                    // Echo back
                    let _ = stream.write_all(&buf[..n]).await;
                });
            }
        });
    });
    
    // Give server time to start
    thread::sleep(Duration::from_millis(500));
    
    // Run TCP benchmark
    let mut group = c.benchmark_group("tcp");
    group.measurement_time(Duration::from_secs(5));
    group.sample_size(10);
    
    let config = TcpConfig::new(
        "127.0.0.1:8081".to_string(),
        Some("hello".to_string()),
        None,
        Some("hello".to_string()),
        Some(10),
        Some(1000),
        Some(30),
        Some(1000),
        false,
    );
    
    group.bench_function("tcp_echo", |b| {
        b.iter(|| {
            let runner = TcpRunner::new(config.clone());
            let rt = Runtime::new().unwrap();
            black_box(rt.block_on(async {
                runner.run().await.unwrap()
            }));
        });
    });
    
    group.finish();
}

// Unix Domain Socket benchmarks (skipped on Windows)
#[cfg(unix)]
fn bench_uds(c: &mut Criterion) {
    use std::os::unix::fs::PermissionsExt;
    
    // Setup a simple UDS echo server
    let rt = Runtime::new().unwrap();
    let socket_path = "/tmp/vibe_benchmark.sock";
    
    // Remove socket if it exists
    let _ = fs::remove_file(socket_path);
    
    // Start UDS server
    let server_handle = thread::spawn(move || {
        rt.block_on(async {
            let listener = UnixListener::bind(socket_path).unwrap();
            
            // Set permissions
            let metadata = fs::metadata(socket_path).unwrap();
            let mut perms = metadata.permissions();
            perms.set_mode(0o666);
            fs::set_permissions(socket_path, perms).unwrap();
            
            println!("UDS server listening on {}", socket_path);
            
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                
                task::spawn(async move {
                    let mut buf = [0; 1024];
                    
                    // Read data
                    let n = stream.read(&mut buf).await.unwrap();
                    
                    // Echo back
                    let _ = stream.write_all(&buf[..n]).await;
                });
            }
        });
    });
    
    // Give server time to start
    thread::sleep(Duration::from_millis(500));
    
    // Run UDS benchmark
    let mut group = c.benchmark_group("unix_domain_socket");
    group.measurement_time(Duration::from_secs(5));
    group.sample_size(10);
    
    let config = UdsConfig::new(
        PathBuf::from(socket_path),
        Some("hello".to_string()),
        None,
        Some("hello".to_string()),
        Some(10),
        Some(1000),
        Some(30),
        Some(1000),
        false,
    );
    
    group.bench_function("uds_echo", |b| {
        b.iter(|| {
            let runner = UdsRunner::new(config.clone());
            let rt = Runtime::new().unwrap();
            black_box(rt.block_on(async {
                runner.run().await.unwrap()
            }));
        });
    });
    
    group.finish();
}

#[cfg(not(unix))]
fn bench_uds(_c: &mut Criterion) {
    // Skip on non-Unix platforms
}

criterion_group!(benches, bench_http, bench_tcp, bench_uds);
criterion_main!(benches);