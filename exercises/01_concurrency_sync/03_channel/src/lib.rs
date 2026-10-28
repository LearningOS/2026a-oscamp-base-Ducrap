/*
Send messages between threads and close channels by releasing every sender.
*/

use std::sync::mpsc;
use std::thread;

/// Create a producer thread that sends each element from items into the channel.
/// The main thread receives all messages and returns them.
pub fn simple_send_recv(items: Vec<String>) -> Vec<String> {
    let (sender, receiver) = mpsc::channel();
    let producer = thread::spawn(move || {
        for item in items {
            sender.send(item).expect("receiver disconnected");
        }
    });
    let messages = receiver.into_iter().collect();
    producer.join().expect("producer thread panicked");
    messages
}

/// Create `n_producers` producer threads, each sending a message in format `"msg from {id}"`.
/// Collect all messages, sort them lexicographically, and return.
///
/// Hint: Use `tx.clone()` to create multiple senders. Note that the original tx must also be dropped.
pub fn multi_producer(n_producers: usize) -> Vec<String> {
    let (sender, receiver) = mpsc::channel();
    let mut producers = Vec::with_capacity(n_producers);
    for id in 0..n_producers {
        let sender = sender.clone();
        producers.push(thread::spawn(move || {
            sender
                .send(format!("msg from {id}"))
                .expect("receiver disconnected");
        }));
    }
    // Only producer-owned senders should keep the receiver open.
    drop(sender);
    let mut messages: Vec<_> = receiver.into_iter().collect();
    for producer in producers {
        producer.join().expect("producer thread panicked");
    }
    messages.sort();
    messages
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_send_recv() {
        let items = vec!["hello".into(), "world".into(), "rust".into()];
        let result = simple_send_recv(items.clone());
        assert_eq!(result, items);
    }

    #[test]
    fn test_simple_empty() {
        let result = simple_send_recv(vec![]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_multi_producer() {
        let result = multi_producer(3);
        assert_eq!(
            result,
            vec![
                "msg from 0".to_string(),
                "msg from 1".to_string(),
                "msg from 2".to_string(),
            ]
        );
    }

    #[test]
    fn test_multi_producer_single() {
        let result = multi_producer(1);
        assert_eq!(result, vec!["msg from 0".to_string()]);
    }
}
