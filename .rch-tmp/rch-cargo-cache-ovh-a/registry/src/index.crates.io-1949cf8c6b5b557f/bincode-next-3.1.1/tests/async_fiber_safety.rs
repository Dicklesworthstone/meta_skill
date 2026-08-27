#![cfg(feature = "async-fiber")]
mod safety_tests {
    use bincode_next::Decode;
    use bincode_next::config;
    use bincode_next::de::read::Reader;
    use bincode_next::decode_async;
    use futures_io::AsyncRead;
    use std::pin::Pin;
    use std::task::Context;
    use std::task::Poll;
    use std::task::RawWaker;
    use std::task::RawWakerVTable;
    use std::task::Waker;

    fn dummy_waker() -> Waker {
        static VTABLE: RawWakerVTable = RawWakerVTable::new(
            |_| RawWaker::new(std::ptr::null(), &VTABLE),
            |_| {},
            |_| {},
            |_| {},
        );
        unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) }
    }

    struct PanickingStruct;

    impl Decode<()> for PanickingStruct {
        fn decode<D: bincode_next::de::Decoder>(
            _decoder: &mut D
        ) -> Result<Self, bincode_next::error::DecodeError> {
            panic!("Intentional panic inside fiber!");
        }
    }

    struct DummyReader;

    impl AsyncRead for DummyReader {
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buf: &mut [u8],
        ) -> Poll<std::io::Result<usize>> {
            Poll::Pending
        }
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_panic_propagation() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();

        let result = std::panic::catch_unwind(|| {
            rt.block_on(async {
                let reader = DummyReader;
                let _decoded: PanickingStruct =
                    decode_async(config::standard(), reader).await.unwrap();
            })
        });

        assert!(result.is_err());
        let err = result.unwrap_err();
        if let Some(s) = err.downcast_ref::<&str>() {
            assert_eq!(*s, "Intentional panic inside fiber!");
        }
    }

    struct SuspendedStruct;

    impl Decode<()> for SuspendedStruct {
        fn decode<D: bincode_next::de::Decoder>(
            decoder: &mut D
        ) -> Result<Self, bincode_next::error::DecodeError> {
            let mut buf = [0u8; 1];
            // This will pend indefinitely with DummyReader, yielding back to the executor
            decoder.reader().read(&mut buf)?;
            Ok(SuspendedStruct)
        }
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_fiber_drop_memory_safety() {
        let reader = DummyReader;
        let mut future = Box::pin(decode_async::<SuspendedStruct, _, _>(
            config::standard(),
            reader,
        ));

        let waker = dummy_waker();
        let mut cx = Context::from_waker(&waker);

        // Execute fiber up to the yield point inside Decoder::read()
        assert!(future.as_mut().poll(&mut cx).is_pending());

        // Now drop the future while the fiber is yielded mid-execution.
        // This exercises BridgeFuture::drop, which drops the mapped FiberContext
        // and its GuardedStack cleanly, preventing segfaults. Locals inside the
        // fiber stack will be leaked as designed (safe in Rust), but the OS-level
        // memory mapped via mmap is appropriately unmapped via Drop on GuardedStack.
        drop(future);
    }
}
