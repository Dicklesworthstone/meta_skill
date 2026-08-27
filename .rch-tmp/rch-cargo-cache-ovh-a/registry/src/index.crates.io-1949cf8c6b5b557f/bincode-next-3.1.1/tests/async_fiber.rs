#![cfg(feature = "async-fiber")]
mod async_fiber_tests {
    use bincode_next::Decode;
    use bincode_next::Encode;
    use bincode_next::config;
    use bincode_next::decode_async;
    use bincode_next::encode_to_vec;
    use futures_io::AsyncRead;
    use std::pin::Pin;
    use std::task::Context;
    use std::task::Poll;

    #[derive(Encode, Decode, PartialEq, Debug, Clone)]
    struct ComplexStruct {
        a: u32,
        b: String,
        c: Vec<u8>,
        d: Vec<String>,
        e: Option<u64>,
    }

    struct ChunkReader {
        data: Vec<u8>,
        chunk_size: usize,
        pos: usize,
    }

    impl AsyncRead for ChunkReader {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut [u8],
        ) -> Poll<std::io::Result<usize>> {
            if self.pos >= self.data.len() || buf.is_empty() {
                return Poll::Ready(Ok(0));
            }
            let to_read = std::cmp::min(self.chunk_size, buf.len());
            let end = std::cmp::min(self.pos + to_read, self.data.len());
            let copied = end - self.pos;
            buf[..copied].copy_from_slice(&self.data[self.pos..end]);
            self.pos = end;
            Poll::Ready(Ok(copied))
        }
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_async_fiber_chunked() {
        let original = ComplexStruct {
            a: 42,
            b: "hello world across multiple fibers!".to_string(),
            c: vec![1, 2, 3, 4, 5, 255, 128, 64, 32],
            d: vec!["foo".to_string(), "bar".to_string(), "baz".to_string()],
            e: Some(9999),
        };

        let encoded = encode_to_vec(&original, config::standard()).unwrap();

        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();

        rt.block_on(async {
            // Try different chunk sizes to force the fiber to yield multiple times
            for chunk_size in [1, 2, 5, 10, 100] {
                let reader = ChunkReader {
                    data: encoded.clone(),
                    chunk_size,
                    pos: 0,
                };

                let decoded: ComplexStruct =
                    decode_async::<ComplexStruct, _, _>(config::standard(), reader)
                        .await
                        .unwrap();
                assert_eq!(original, decoded, "Failed at chunk size {}", chunk_size);
            }
        });
    }
}
