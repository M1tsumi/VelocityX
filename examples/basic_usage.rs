//! Basic usage example for VelocityX
//! 
//! This example demonstrates the basic usage of the MPMC queue

use velocityx::queue::MpmcQueue;
use std::thread;
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("VelocityX Basic Usage Example");
    println!("============================");
    
    // Create a bounded MPMC queue
    let queue: Arc<MpmcQueue<i32>> = Arc::new(MpmcQueue::new(1000));
    
    // Basic push/pop operations
    println!("\n1. Basic Operations:");
    queue.push(42)?;
    queue.push(24)?;
    
    let value1 = queue.pop();
    let value2 = queue.pop();
    
    println!("   Pushed: 42, 24");
    println!("   Popped: {:?}, {:?}", value1, value2);
    
    // Multi-producer scenario
    println!("\n2. Multi-Producer:");
    let producer_handles: Vec<_> = (0..3)
        .map(|i| {
            let queue: Arc<MpmcQueue<i32>> = Arc::clone(&queue);
            thread::spawn(move || {
                for j in 0..10 {
                    let value = i * 10 + j;
                    queue.push(value).unwrap();
                    println!("   Producer {} pushed {}", i, value);
                }
            })
        })
        .collect();
    
    // Wait for all producers to finish
    for handle in producer_handles {
        handle.join().unwrap();
    }
    
    // Multi-consumer scenario
    println!("\n3. Multi-Consumer:");
    let consumer_handles: Vec<_> = (0..2)
        .map(|i| {
            let queue: Arc<MpmcQueue<i32>> = Arc::clone(&queue);
            thread::spawn(move || {
                let mut count = 0;
                while let Some(value) = queue.pop() {
                    println!("   Consumer {} got {}", i, value);
                    count += 1;
                }
                count
            })
        })
        .collect();
    
    // Wait for all consumers to finish
    let mut total_consumed = 0;
    for handle in consumer_handles {
        total_consumed += handle.join().unwrap();
    }
    
    println!("\n4. Results:");
    println!("   Total items consumed: {}", total_consumed);
    println!("   Queue size: {}", queue.len());
    
    println!("\n✅ Basic usage example completed successfully!");
    Ok(())
}
