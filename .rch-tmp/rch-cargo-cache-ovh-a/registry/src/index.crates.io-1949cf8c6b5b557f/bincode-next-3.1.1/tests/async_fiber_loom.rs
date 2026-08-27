#![cfg(all(feature = "async-fiber", loom))]
mod loom_tests {
    use bincode_next::config;
    use bincode_next::decode_async;
    use futures_io::AsyncRead;
    use loom::thread;
    use std::pin::Pin;
    use std::task::Context;
    use std::task::Poll;
    use std::task::RawWaker;
    use std::task::RawWakerVTable;
    use std::task::Waker;

    // A mock AsyncRead that yields exactly once, simulating an IO delay.
    struct YieldReader {
        data: Vec<u8>,
        yielded: bool,
    }

    impl AsyncRead for YieldReader {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut [u8],
        ) -> Poll<std::io::Result<usize>> {
            if !self.yielded {
                self.yielded = true;
                Poll::Pending
            } else {
                let to_read = std::cmp::min(self.data.len(), buf.len());
                buf[..to_read].copy_from_slice(&self.data[..to_read]);
                Poll::Ready(Ok(to_read))
            }
        }
    }

    // Dummy waker
    fn dummy_waker() -> Waker {
        static VTABLE: RawWakerVTable = RawWakerVTable::new(
            |_| RawWaker::new(std::ptr::null(), &VTABLE),
            |_| {},
            |_| {},
            |_| {},
        );
        unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) }
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_fiber_thread_migration() {
        loom::model(|| {
            let encoded = bincode_next::encode_to_vec(&42u32, config::standard()).unwrap();
            let reader = YieldReader {
                data: encoded,
                yielded: false,
            };

            // Initialize the future
            let mut f = Box::pin(decode_async::<u32, _, _>(config::standard(), reader));
            let waker = dummy_waker();
            let mut cx = Context::from_waker(&waker);

            // First poll triggers the fiber to start, read 0 bytes, yield Pending.
            assert!(f.as_mut().poll(&mut cx).is_pending());

            // Move the future to another thread and finish it there
            thread::spawn(move || {
                let waker = dummy_waker();
                let mut cx = Context::from_waker(&waker);

                // Second poll resumes the fiber on a different thread
                let Poll::Ready(Ok(decoded)) = f.as_mut().poll(&mut cx) else {
                    panic!("Expected Ready(Ok)");
                };

                assert_eq!(decoded, 42);
            })
            .join()
            .unwrap();
        });
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_fiber_cancellation() {
        loom::model(|| {
            let encoded = bincode_next::encode_to_vec(&42u32, config::standard()).unwrap();
            let reader = YieldReader {
                data: encoded,
                yielded: false,
            };

            // Initialize the future
            let mut f = Box::pin(decode_async::<u32, _, _>(config::standard(), reader));
            let waker = dummy_waker();
            let mut cx = Context::from_waker(&waker);

            // First poll triggers the fiber to start, read 0 bytes, yield Pending.
            assert!(f.as_mut().poll(&mut cx).is_pending());

            // Future is dropped here, which should clean up the fiber resources cleanly
            // even though it was yielded.
            drop(f);
        });
    }
}
