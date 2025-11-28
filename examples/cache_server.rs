//! Lock-free caching server example
//!
//! This example demonstrates using a concurrent hash map to build a high-performance
//! caching server that can handle multiple concurrent read and write operations
//! without blocking on reads.

use velocityx::map::ConcurrentHashMap;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
struct CacheEntry {
    data: String,
    timestamp: u64,
    access_count: u64,
    ttl_seconds: u64,
}

impl CacheEntry {
    fn new(data: String, ttl_seconds: u64) -> Self {
        Self {
            data,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            access_count: 0,
            ttl_seconds,
        }
    }

    fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        now > self.timestamp + self.ttl_seconds
    }

    fn access(&mut self) -> &str {
        self.access_count += 1;
        &self.data
    }
}

#[derive(Debug)]
struct CacheStats {
    hits: u64,
    misses: u64,
    sets: u64,
    deletes: u64,
    evictions: u64,
}

impl CacheStats {
    fn new() -> Self {
        Self {
            hits: 0,
            misses: 0,
            sets: 0,
            deletes: 0,
            evictions: 0,
        }
    }

    fn hit_rate(&self) -> f64 {
        if self.hits + self.misses == 0 {
            0.0
        } else {
            self.hits as f64 / (self.hits + self.misses) as f64 * 100.0
        }
    }
}

struct CacheServer {
    map: Arc<ConcurrentHashMap<String, CacheEntry>>,
    stats: Arc<std::sync::Mutex<CacheStats>>,
    max_size: usize,
    cleanup_interval: Duration,
}

impl CacheServer {
    fn new(max_size: usize, cleanup_interval: Duration) -> Self {
        Self {
            map: Arc::new(ConcurrentHashMap::new()),
            stats: Arc::new(std::sync::Mutex::new(CacheStats::new())),
            max_size,
            cleanup_interval,
        }
    }

    fn get(&self, key: &str) -> Option<String> {
        if let Some(entry) = self.map.get(key) {
            // Note: In a real implementation, we'd need to handle access_count atomically
            // For this example, we'll just return the data
            if !entry.is_expired() {
                if let Ok(mut stats) = self.stats.lock() {
                    stats.hits += 1;
                }
                Some(entry.data.clone())
            } else {
                // Entry expired, remove it
                self.map.remove(key);
                if let Ok(mut stats) = self.stats.lock() {
                    stats.misses += 1;
                    stats.evictions += 1;
                }
                None
            }
        } else {
            if let Ok(mut stats) = self.stats.lock() {
                stats.misses += 1;
            }
            None
        }
    }

    fn set(&self, key: String, value: String, ttl_seconds: u64) {
        // Check if we need to evict (simplified - in practice would use LRU)
        if self.map.len() >= self.max_size {
            self.evict_random();
        }

        let entry = CacheEntry::new(value, ttl_seconds);
        self.map.insert(key.clone(), entry);
        
        if let Ok(mut stats) = self.stats.lock() {
            stats.sets += 1;
        }
    }

    fn delete(&self, key: &str) -> bool {
        let result = self.map.remove(key).is_some();
        if result {
            if let Ok(mut stats) = self.stats.lock() {
                stats.deletes += 1;
            }
        }
        result
    }

    fn evict_random(&self) {
        // Simple eviction strategy - in practice would use LRU or similar
        let keys_to_remove: Vec<String> = self.map
            .capacity()
            .into_iter()
            .filter_map(|i| {
                // This is a simplified approach - real implementation would be more sophisticated
                if i % 100 == 0 && self.map.len() > self.max_size * 9 / 10 {
                    Some(format!("key_{}", i))
                } else {
                    None
                }
            })
            .take(self.max_size / 10)
            .collect();

        for key in keys_to_remove {
            self.map.remove(&key);
            if let Ok(mut stats) = self.stats.lock() {
                stats.evictions += 1;
            }
        }
    }

    fn cleanup_expired(&self) {
        let mut keys_to_remove = vec![];
        
        // This is a simplified cleanup - real implementation would be more efficient
        for i in 0..self.map.capacity() {
            let key = format!("key_{}", i);
            if let Some(entry) = self.map.get(&key) {
                if entry.is_expired() {
                    keys_to_remove.push(key);
                }
            }
        }

        for key in keys_to_remove {
            self.map.remove(&key);
            if let Ok(mut stats) = self.stats.lock() {
                stats.evictions += 1;
            }
        }
    }

    fn get_stats(&self) -> CacheStats {
        self.stats.lock().unwrap().clone()
    }

    fn start_cleanup_thread(&self) -> thread::JoinHandle<()> {
        let map = Arc::clone(&self.map);
        let stats = Arc::clone(&self.stats);
        let cleanup_interval = self.cleanup_interval;

        thread::spawn(move || {
            loop {
                thread::sleep(cleanup_interval);
                
                let mut keys_to_remove = vec![];
                for i in 0..map.capacity() {
                    let key = format!("key_{}", i);
                    if let Some(entry) = map.get(&key) {
                        if entry.is_expired() {
                            keys_to_remove.push(key);
                        }
                    }
                }

                for key in keys_to_remove {
                    map.remove(&key);
                    if let Ok(mut stats) = stats.lock() {
                        stats.evictions += 1;
                    }
                }
            }
        })
    }
}

fn main() {
    println!("🗄️  Lock-Free Cache Server Example");
    println!("==================================");

    // Configuration
    let num_readers = 8;
    let num_writers = 4;
    let operations_per_thread = 10_000;
    let cache_size = 10_000;
    let cleanup_interval = Duration::from_secs(1);

    println!("Configuration:");
    println!("  Cache size: {}", cache_size);
    println!("  Reader threads: {}", num_readers);
    println!("  Writer threads: {}", num_writers);
    println!("  Operations per thread: {}", operations_per_thread);
    println!("  Total operations: {}", (num_readers + num_writers) * operations_per_thread);
    println!("  Cleanup interval: {:?}\n", cleanup_interval);

    let cache = Arc::new(CacheServer::new(cache_size, cleanup_interval));
    
    // Start cleanup thread
    let cleanup_handle = cache.start_cleanup_thread();

    let start_time = Instant::now();
    let barrier = Arc::new(Barrier::new(num_readers + num_writers));

    // Spawn reader threads
    let mut reader_handles = vec![];
    for reader_id in 0..num_readers {
        let cache = Arc::clone(&cache);
        let barrier = Arc::clone(&barrier);
        let handle = thread::spawn(move || {
            barrier.wait();
            
            let mut hits = 0;
            let mut misses = 0;
            
            for i in 0..operations_per_thread {
                let key = format!("key_{}", i % cache_size);
                
                match cache.get(&key) {
                    Some(_value) => hits += 1,
                    None => misses += 1,
                }

                // Occasionally read a random key
                if i % 100 == 0 {
                    let random_key = format!("key_{}", (i * 7) % cache_size);
                    let _ = cache.get(&random_key);
                }
            }

            (hits, misses)
        });
        reader_handles.push(handle);
    }

    // Spawn writer threads
    let mut writer_handles = vec![];
    for writer_id in 0..num_writers {
        let cache = Arc::clone(&cache);
        let barrier = Arc::clone(&barrier);
        let handle = thread::spawn(move || {
            barrier.wait();
            
            let mut sets = 0;
            let mut deletes = 0;
            
            for i in 0..operations_per_thread {
                let key = format!("key_{}", i % cache_size);
                let value = format!("Data from writer {} at operation {}", writer_id, i);
                let ttl = 10 + (i % 20); // TTL between 10-30 seconds

                cache.set(key.clone(), value, ttl);
                sets += 1;

                // Occasionally delete a key
                if i % 200 == 0 {
                    let delete_key = format!("key_{}", (i * 3) % cache_size);
                    if cache.delete(&delete_key) {
                        deletes += 1;
                    }
                }

                // Occasionally update an existing key
                if i % 150 == 0 {
                    let update_key = format!("key_{}", (i * 5) % cache_size);
                    let update_value = format!("Updated data from writer {} at operation {}", writer_id, i);
                    cache.set(update_key, update_value, 15);
                    sets += 1;
                }
            }

            (sets, deletes)
        });
        writer_handles.push(handle);
    }

    // Collect results
    let mut total_hits = 0;
    let mut total_misses = 0;
    for handle in reader_handles {
        let (hits, misses) = handle.join().unwrap();
        total_hits += hits;
        total_misses += misses;
    }

    let mut total_sets = 0;
    let mut total_deletes = 0;
    for handle in writer_handles {
        let (sets, deletes) = handle.join().unwrap();
        total_sets += sets;
        total_deletes += deletes;
    }

    let elapsed = start_time.elapsed();
    let final_stats = cache.get_stats();

    println!("\n📊 Results:");
    println!("  Total operations: {}", operations_per_thread * (num_readers + num_writers));
    println!("  Elapsed time: {:?}", elapsed);
    println!("  Operations per second: {:.0}", 
             operations_per_thread as f64 * (num_readers + num_writers) as f64 / elapsed.as_secs_f64());

    println!("\n🎯 Cache Statistics:");
    println!("  Cache hits: {}", final_stats.hits);
    println!("  Cache misses: {}", final_stats.misses);
    println!("  Cache sets: {}", final_stats.sets);
    println!("  Cache deletes: {}", final_stats.deletes);
    println!("  Cache evictions: {}", final_stats.evictions);
    println!("  Hit rate: {:.2}%", final_stats.hit_rate());
    println!("  Current cache size: {}", cache.map.len());
    println!("  Cache capacity: {}", cache.map.capacity());

    println!("\n👥 Thread Statistics:");
    println!("  Reader hits: {}", total_hits);
    println!("  Reader misses: {}", total_misses);
    println!("  Writer sets: {}", total_sets);
    println!("  Writer deletes: {}", total_deletes);

    // Performance analysis
    let total_ops = final_stats.hits + final_stats.misses + final_stats.sets + final_stats.deletes;
    let ops_per_sec = total_ops as f64 / elapsed.as_secs_f64();
    
    println!("\n⚡ Performance:");
    println!("  Total operations: {}", total_ops);
    println!("  Operations per second: {:.0}", ops_per_sec);
    println!("  Average operation time: {:.2} μs", 1_000_000.0 / ops_per_sec);

    // Cache efficiency analysis
    let hit_rate = final_stats.hit_rate();
    if hit_rate > 80.0 {
        println!("🎯 Excellent cache hit rate!");
    } else if hit_rate > 60.0 {
        println!("👍 Good cache hit rate!");
    } else {
        println!("⚠️  Cache hit rate could be improved.");
    }

    println!("\n✅ Cache server test completed successfully!");

    // Stop cleanup thread (in a real implementation, you'd have a graceful shutdown)
    drop(cleanup_handle);
}
