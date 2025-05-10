# Vibe - Rust Performance Benchmark Tool

Vibe is a high-performance benchmark tool for testing HTTP, TCP, and Unix Domain Socket (UDS) server APIs. It allows you to simulate concurrent connections and measure performance metrics such as requests per second, response times, and throughput.

## Features

- Benchmark HTTP servers with customizable methods, headers, and body content
- Benchmark TCP servers with configurable data payloads
- Benchmark Unix Domain Socket servers
- Set concurrency level, number of requests, and test duration
- Detailed performance reports including latency percentiles (p50, p90, p95, p99)
- Colorful progress display with ETA
- JSON output option for programmatic analysis

## Installation

```bash
cargo install --path .
```

## Usage

### HTTP Benchmarking

```bash
# Basic GET request with 10 concurrent connections and 1000 total requests
vibe http http://example.com -c 10 -r 1000

# POST request with custom headers and body
vibe http https://api.example.com/users -m POST \
  -h "Content-Type: application/json" \
  -h "Authorization: Bearer token123" \
  -b '{"name": "Test User", "email": "test@example.com"}'

# Benchmark for 30 seconds with connection keep-alive
vibe http http://example.com -c 50 -d 30 --keep-alive
```

### TCP Benchmarking

```bash
# Simple TCP benchmark
vibe tcp 127.0.0.1:6379 -d "PING\r\n" -e "PONG"

# Benchmark with data from a file
vibe tcp 127.0.0.1:5000 --data-file ./payload.bin -c 20
```

### Unix Domain Socket Benchmarking

```bash
# Benchmark a UDS server
vibe uds /tmp/app.sock -d "GET /stats" -e "ok"
```

### Common Options

- `-c, --concurrency`: Number of concurrent connections (default: 1)
- `-r, --requests`: Total number of requests (default: 100)
- `-d, --duration`: Duration of the test in seconds (default: 10)
- `-t, --timeout`: Timeout for each request in milliseconds (default: 30000)
- `--keep-alive`: Keep connections alive
- `--output`: Output format (text, json)

## Performance Tips

1. For high concurrency tests, increase your system's file descriptor limits
2. Use `--keep-alive` for HTTP benchmarks to reuse connections
3. Monitor both client and server CPU/memory during tests

## License

MIT