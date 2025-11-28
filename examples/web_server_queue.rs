//! Web Server Producer-Consumer Example
//!
//! This example demonstrates how to use VelocityX MPMC queues in a real-world
//! web server scenario with multiple producer threads (request handlers) and
//! consumer threads (worker processors).

use velocityx::queue::mpmc::{MpmcQueue, UnboundedMpmcQueue};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::collections::HashMap;

/// Represents an HTTP request to be processed
#[derive(Debug, Clone)]
struct HttpRequest {
    id: u64,
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
    timestamp: u64,
}

impl HttpRequest {
    fn new(id: u64, method: &str, path: &str) -> Self {
        Self {
            id,
            method: method.to_string(),
            path: path.to_string(),
            headers: HashMap::new(),
            body: Vec::new(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        }
    }
}

/// Represents an HTTP response
#[derive(Debug, Clone)]
struct HttpResponse {
    request_id: u64,
    status_code: u16,
    headers: HashMap<String, String>,
    body: Vec<u8>,
    processing_time: Duration,
}

impl HttpResponse {
    fn new(request_id: u64, status_code: u16, processing_time: Duration) -> Self {
        Self {
            request_id,
            status_code,
            headers: HashMap::new(),
            body: Vec::new(),
            processing_time,
        }
    }
}

/// Request processor that simulates actual HTTP processing
struct RequestProcessor {
    /// Processing time simulation (in milliseconds)
    base_processing_time: u64,
    /// Random variation in processing time
    processing_variance: u64,
}

impl RequestProcessor {
    fn new(base_time: u64, variance: u64) -> Self {
        Self {
            base_processing_time: base_time,
            processing_variance: variance,
        }
    }

    fn process_request(&self, request: &HttpRequest) -> HttpResponse {
        let start = Instant::now();
        
        // Simulate processing based on request path
        let processing_time = match request.path.as_str() {
            "/api/fast" => self.base_processing_time / 2,
            "/api/slow" => self.base_processing_time * 2,
            "/api/heavy" => self.base_processing_time * 5,
            _ => self.base_processing_time,
        };
        
        // Add some random variation
        let variation = (request.id % self.processing_variance) as u64;
        let total_time = processing_time + variation;
        
        thread::sleep(Duration::from_millis(total_time));
        
        let actual_duration = start.elapsed();
        
        // Determine status code based on processing
        let status_code = if actual_duration > Duration::from_millis(self.base_processing_time * 3) {
            503 // Service Unavailable
        } else if request.path.contains("/error") {
            500 // Internal Server Error
        } else {
            200 // OK
        };
        
        HttpResponse::new(request.id, status_code, actual_duration)
    }
}

/// Web server statistics
#[derive(Debug, Default)]
struct ServerStats {
    requests_received: u64,
    requests_processed: u64,
    requests_failed: u64,
    total_processing_time: Duration,
    avg_response_time: Duration,
    peak_queue_size: usize,
}

impl ServerStats {
    fn update_processed(&mut self, processing_time: Duration, queue_size: usize) {
        self.requests_processed += 1;
        self.total_processing_time += processing_time;
        self.avg_response_time = self.total_processing_time / self.requests_processed as u32;
        self.peak_queue_size = self.peak_queue_size.max(queue_size);
    }

    fn update_failed(&mut self) {
        self.requests_failed += 1;
    }

    fn success_rate(&self) -> f64 {
        if self.requests_processed + self.requests_failed == 0 {
            0.0
        } else {
            self.requests_processed as f64 / (self.requests_processed + self.requests_failed) as f64 * 100.0
        }
    }
}

/// Web server using bounded MPMC queue
struct BoundedWebServer {
    request_queue: Arc<MpmcQueue<HttpRequest>>,
    response_queue: Arc<MpmcQueue<HttpResponse>>,
    processor: Arc<RequestProcessor>,
    stats: Arc<Mutex<ServerStats>>,
}

impl BoundedWebServer {
    fn new(queue_capacity: usize, processor: RequestProcessor) -> Self {
        Self {
            request_queue: Arc::new(MpmcQueue::new(queue_capacity)),
            response_queue: Arc::new(MpmcQueue::new(queue_capacity)),
            processor: Arc::new(processor),
            stats: Arc::new(Mutex::new(ServerStats::default())),
        }
    }

    fn start_request_handlers(&self, num_handlers: usize, requests_per_handler: usize) -> Vec<thread::JoinHandle<()>> {
        let mut handles = Vec::new();
        
        for handler_id in 0..num_handlers {
            let request_queue = Arc::clone(&self.request_queue);
            let stats = Arc::clone(&self.stats);
            
            let handle = thread::spawn(move || {
                for i in 0..requests_per_handler {
                    let request_id = handler_id * requests_per_handler + i;
                    let paths = ["/api/fast", "/api/slow", "/api/heavy", "/api/normal", "/api/error"];
                    let path = paths[request_id % paths.len()];
                    let method = if request_id % 3 == 0 { "POST" } else { "GET" };
                    
                    let request = HttpRequest::new(request_id as u64, method, path);
                    
                    // Try to push request to queue (may fail if queue is full)
                    match request_queue.push(request) {
                        Ok(()) => {
                            stats.lock().unwrap().requests_received += 1;
                        }
                        Err(_) => {
                            // Queue full, simulate dropping the request
                            stats.lock().unwrap().requests_failed += 1;
                        }
                    }
                    
                    // Simulate request arrival pattern
                    if i % 10 == 0 {
                        thread::sleep(Duration::from_millis(1));
                    }
                }
            });
            
            handles.push(handle);
        }
        
        handles
    }

    fn start_workers(&self, num_workers: usize) -> Vec<thread::JoinHandle<()>> {
        let mut handles = Vec::new();
        
        for _ in 0..num_workers {
            let request_queue = Arc::clone(&self.request_queue);
            let response_queue = Arc::clone(&self.response_queue);
            let processor = Arc::clone(&self.processor);
            let stats = Arc::clone(&self.stats);
            
            let handle = thread::spawn(move || {
                loop {
                    match request_queue.pop() {
                        Some(request) => {
                            let queue_size = request_queue.len();
                            let response = processor.process_request(&request);
                            
                            // Try to push response (may fail if queue is full)
                            if response_queue.push(response).is_ok() {
                                stats.lock().unwrap().update_processed(
                                    response.processing_time,
                                    queue_size
                                );
                            } else {
                                stats.lock().unwrap().update_failed();
                            }
                        }
                        None => {
                            // No requests available, check if we should exit
                            // In a real server, you'd have a shutdown signal
                            thread::sleep(Duration::from_millis(1));
                            continue;
                        }
                    }
                }
            });
            
            handles.push(handle);
        }
        
        handles
    }

    fn start_response_handlers(&self, num_handlers: usize) -> Vec<thread::JoinHandle<()>> {
        let mut handles = Vec::new();
        
        for _ in 0..num_handlers {
            let response_queue = Arc::clone(&self.response_queue);
            
            let handle = thread::spawn(move || {
                loop {
                    match response_queue.pop() {
                        Some(response) => {
                            // In a real server, you'd send the response back to the client
                            // Here we just simulate processing the response
                            if response.status_code >= 400 {
                                // Log error responses
                                println!("Error response: {} for request {}", 
                                        response.status_code, response.request_id);
                            }
                        }
                        None => {
                            thread::sleep(Duration::from_millis(1));
                            continue;
                        }
                    }
                }
            });
            
            handles.push(handle);
        }
        
        handles
    }

    fn get_stats(&self) -> ServerStats {
        self.stats.lock().unwrap().clone()
    }
}

/// Web server using unbounded MPMC queue
struct UnboundedWebServer {
    request_queue: Arc<UnboundedMpmcQueue<HttpRequest>>,
    response_queue: Arc<UnboundedMpmcQueue<HttpResponse>>,
    processor: Arc<RequestProcessor>,
    stats: Arc<Mutex<ServerStats>>,
}

impl UnboundedWebServer {
    fn new(processor: RequestProcessor) -> Self {
        Self {
            request_queue: Arc::new(UnboundedMpmcQueue::new()),
            response_queue: Arc::new(UnboundedMpmcQueue::new()),
            processor: Arc::new(processor),
            stats: Arc::new(Mutex::new(ServerStats::default())),
        }
    }

    fn start_request_handlers(&self, num_handlers: usize, requests_per_handler: usize) -> Vec<thread::JoinHandle<()>> {
        let mut handles = Vec::new();
        
        for handler_id in 0..num_handlers {
            let request_queue = Arc::clone(&self.request_queue);
            let stats = Arc::clone(&self.stats);
            
            let handle = thread::spawn(move || {
                for i in 0..requests_per_handler {
                    let request_id = handler_id * requests_per_handler + i;
                    let paths = ["/api/fast", "/api/slow", "/api/heavy", "/api/normal", "/api/error"];
                    let path = paths[request_id % paths.len()];
                    let method = if request_id % 3 == 0 { "POST" } else { "GET" };
                    
                    let request = HttpRequest::new(request_id as u64, method, path);
                    
                    // Push request to queue (always succeeds for unbounded)
                    request_queue.push(request).unwrap();
                    stats.lock().unwrap().requests_received += 1;
                    
                    // Simulate request arrival pattern
                    if i % 10 == 0 {
                        thread::sleep(Duration::from_millis(1));
                    }
                }
            });
            
            handles.push(handle);
        }
        
        handles
    }

    fn start_workers(&self, num_workers: usize) -> Vec<thread::JoinHandle<()>> {
        let mut handles = Vec::new();
        
        for _ in 0..num_workers {
            let request_queue = Arc::clone(&self.request_queue);
            let response_queue = Arc::clone(&self.response_queue);
            let processor = Arc::clone(&self.processor);
            let stats = Arc::clone(&self.stats);
            
            let handle = thread::spawn(move || {
                loop {
                    match request_queue.pop() {
                        Some(request) => {
                            let queue_size = request_queue.len();
                            let response = processor.process_request(&request);
                            
                            // Push response (always succeeds for unbounded)
                            response_queue.push(response).unwrap();
                            stats.lock().unwrap().update_processed(
                                response.processing_time,
                                queue_size
                            );
                        }
                        None => {
                            thread::sleep(Duration::from_millis(1));
                            continue;
                        }
                    }
                }
            });
            
            handles.push(handle);
        }
        
        handles
    }

    fn start_response_handlers(&self, num_handlers: usize) -> Vec<thread::JoinHandle<()>> {
        let mut handles = Vec::new();
        
        for _ in 0..num_handlers {
            let response_queue = Arc::clone(&self.response_queue);
            
            let handle = thread::spawn(move || {
                loop {
                    match response_queue.pop() {
                        Some(response) => {
                            if response.status_code >= 400 {
                                println!("Error response: {} for request {}", 
                                        response.status_code, response.request_id);
                            }
                        }
                        None => {
                            thread::sleep(Duration::from_millis(1));
                            continue;
                        }
                    }
                }
            });
            
            handles.push(handle);
        }
        
        handles
    }

    fn get_stats(&self) -> ServerStats {
        self.stats.lock().unwrap().clone()
    }
}

fn run_bounded_server_test() {
    println!("🚀 Testing Bounded MPMC Queue Web Server");
    println!("==========================================");

    let processor = RequestProcessor::new(10, 5); // 10ms base, 5ms variance
    let server = BoundedWebServer::new(100, processor);
    
    let num_handlers = 4;
    let num_workers = 3;
    let num_response_handlers = 2;
    let requests_per_handler = 100;
    
    println!("Configuration:");
    println!("  Request queue capacity: 100");
    println!("  Request handlers: {}", num_handlers);
    println!("  Workers: {}", num_workers);
    println!("  Response handlers: {}", num_response_handlers);
    println!("  Requests per handler: {}", requests_per_handler);
    println!("  Total requests: {}", num_handlers * requests_per_handler);
    
    let start_time = Instant::now();
    
    // Start all worker threads
    let worker_handles = server.start_workers(num_workers);
    let response_handles = server.start_response_handlers(num_response_handlers);
    
    // Start request handlers
    let handler_handles = server.start_request_handlers(num_handlers, requests_per_handler);
    
    // Wait for all request handlers to finish
    for handle in handler_handles {
        handle.join().unwrap();
    }
    
    // Give workers time to process remaining requests
    thread::sleep(Duration::from_millis(1000));
    
    let elapsed = start_time.elapsed();
    let stats = server.get_stats();
    
    println!("\n📊 Bounded Queue Results:");
    println!("  Total time: {:?}", elapsed);
    println!("  Requests received: {}", stats.requests_received);
    println!("  Requests processed: {}", stats.requests_processed);
    println!("  Requests failed: {}", stats.requests_failed);
    println!("  Success rate: {:.2}%", stats.success_rate());
    println!("  Average response time: {:?}", stats.avg_response_time);
    println!("  Peak queue size: {}", stats.peak_queue_size);
    println!("  Requests per second: {:.2}", 
             stats.requests_processed as f64 / elapsed.as_secs_f64());
    
    // Note: In a real implementation, you'd have proper shutdown handling
}

fn run_unbounded_server_test() {
    println!("\n🚀 Testing Unbounded MPMC Queue Web Server");
    println!("============================================");

    let processor = RequestProcessor::new(10, 5); // 10ms base, 5ms variance
    let server = UnboundedWebServer::new(processor);
    
    let num_handlers = 4;
    let num_workers = 3;
    let num_response_handlers = 2;
    let requests_per_handler = 100;
    
    println!("Configuration:");
    println!("  Request queue: Unbounded");
    println!("  Request handlers: {}", num_handlers);
    println!("  Workers: {}", num_workers);
    println!("  Response handlers: {}", num_response_handlers);
    println!("  Requests per handler: {}", requests_per_handler);
    println!("  Total requests: {}", num_handlers * requests_per_handler);
    
    let start_time = Instant::now();
    
    // Start all worker threads
    let worker_handles = server.start_workers(num_workers);
    let response_handles = server.start_response_handlers(num_response_handlers);
    
    // Start request handlers
    let handler_handles = server.start_request_handlers(num_handlers, requests_per_handler);
    
    // Wait for all request handlers to finish
    for handle in handler_handles {
        handle.join().unwrap();
    }
    
    // Give workers time to process remaining requests
    thread::sleep(Duration::from_millis(1000));
    
    let elapsed = start_time.elapsed();
    let stats = server.get_stats();
    
    println!("\n📊 Unbounded Queue Results:");
    println!("  Total time: {:?}", elapsed);
    println!("  Requests received: {}", stats.requests_received);
    println!("  Requests processed: {}", stats.requests_processed);
    println!("  Requests failed: {}", stats.requests_failed);
    println!("  Success rate: {:.2}%", stats.success_rate());
    println!("  Average response time: {:?}", stats.avg_response_time);
    println!("  Peak queue size: {}", stats.peak_queue_size);
    println!("  Requests per second: {:.2}", 
             stats.requests_processed as f64 / elapsed.as_secs_f64());
    
    // Note: In a real implementation, you'd have proper shutdown handling
}

fn main() {
    println!("🌐 Web Server Producer-Consumer Example");
    println!("=======================================");
    println!("Demonstrating VelocityX MPMC queues in a realistic web server scenario\n");
    
    run_bounded_server_test();
    run_unbounded_server_test();
    
    println!("\n✅ Web server example completed successfully!");
    println!("\nKey Insights:");
    println!("• Bounded queues provide memory predictability but may drop requests under load");
    println!("• Unbounded queues handle bursty traffic better but use more memory");
    println!("• Both variants provide excellent throughput and low latency");
    println!("• Cache-line aligned atomic operations prevent false sharing");
    println!("• Proper memory ordering ensures thread safety without locks");
}
