#![cfg(feature = "async-fiber")]
use bincode_next::Encode;
use bincode_next::config;
use bincode_next::decode_async;
use futures_io::AsyncRead;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;
use std::time::Instant;
use tokio::task::JoinHandle;

#[derive(Encode, bincode_next::Decode, PartialEq, Debug, Clone)]
struct BenchPayload {
    id: u64,
    data: String,
}

struct PartialReader {
    data: Vec<u8>,
    yields: usize,
    pos: usize,
}

impl AsyncRead for PartialReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        if self.yields > 0 {
            self.yields -= 1;
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }

        let to_copy = std::cmp::min(self.data.len() - self.pos, buf.len());
        if to_copy == 0 {
            return Poll::Ready(Ok(0));
        }

        buf[..to_copy].copy_from_slice(&self.data[self.pos..self.pos + to_copy]);
        self.pos += to_copy;
        Poll::Ready(Ok(to_copy))
    }
}

async fn run_worker(
    encoded: Vec<u8>
) -> core::result::Result<(), bincode_next::error::DecodeError> {
    let reader = PartialReader {
        data: encoded,
        yields: 1, // Yield once to simulate async I/O wait and context switching
        pos: 0,
    };
    let _decoded: BenchPayload = decode_async(config::standard(), reader).await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[cfg_attr(miri, ignore)]
async fn test_high_concurrency() {
    let payload = BenchPayload {
        id: 123456789,
        data: "High concurrency testing payload".to_string(),
    };
    let encoded = bincode_next::encode_to_vec(&payload, config::standard()).unwrap();

    let total_tasks: usize = 5_000_000;
    let batch_size: usize = 50_000;
    println!(
        "Processing {} tasks in batches of {} ...",
        total_tasks, batch_size,
    );

    let start = Instant::now();
    let mut completed: usize = 0;
    let mut successes: usize = 0;
    let mut errors: usize = 0;

    while completed < total_tasks {
        let this_batch = std::cmp::min(batch_size, total_tasks - completed);
        let mut handles: Vec<
            JoinHandle<core::result::Result<(), bincode_next::error::DecodeError>>,
        > = Vec::with_capacity(this_batch);
        for _ in 0..this_batch {
            let enc_clone = encoded.clone();
            handles.push(tokio::spawn(async move { run_worker(enc_clone).await }));
        }
        for handle in handles {
            match handle.await.unwrap() {
                | Ok(_) => successes += 1,
                | Err(_) => errors += 1,
            }
        }
        completed += this_batch;
    }

    let elapsed = start.elapsed();
    println!(
        "Processed {} tasks in {:.2?} ({:.2} tasks/sec). Success: {}, Errors: {}",
        total_tasks,
        elapsed,
        total_tasks as f64 / elapsed.as_secs_f64(),
        successes,
        errors
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[cfg_attr(miri, ignore)]
async fn test_high_concurrency_no_batching() {
    let payload = BenchPayload {
        id: 123456789,
        data: "High concurrency testing payload".to_string(),
    };
    let encoded = bincode_next::encode_to_vec(&payload, config::standard()).unwrap();

    let concurrency = 5_000_000;
    println!(
        "Spawning {} concurrent parsing tasks (no batching)...",
        concurrency
    );

    let mut handles = Vec::with_capacity(concurrency);

    let start = Instant::now();

    for _ in 0..concurrency {
        let enc_clone = encoded.clone();
        handles.push(tokio::spawn(async move { run_worker(enc_clone).await }));
    }

    let mut successes: usize = 0;
    let mut errors: usize = 0;
    for handle in handles {
        match handle.await.unwrap() {
            | Ok(_) => successes += 1,
            | Err(_) => errors += 1,
        }
    }

    let elapsed = start.elapsed();
    println!(
        "Processed {} tasks (no batching) in {:.2?} ({:.2} tasks/sec). Success: {}, Errors: {}",
        concurrency,
        elapsed,
        concurrency as f64 / elapsed.as_secs_f64(),
        successes,
        errors
    );
}
