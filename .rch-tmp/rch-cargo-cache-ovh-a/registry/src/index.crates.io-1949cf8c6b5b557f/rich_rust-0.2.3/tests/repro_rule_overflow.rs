use rich_rust::console::Console;

#[test]
fn test_rule_truncates_long_title() {
    // We strictly enforce width.
    // Title is 20 chars, console is 10.
    // Should truncate to fit.

    use std::io::Write;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct SharedBuffer(Arc<Mutex<Vec<u8>>>);

    impl Write for SharedBuffer {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().write(buf)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.0.lock().unwrap().flush()
        }
    }

    let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
    let console = Console::builder()
        .width(10)
        .file(Box::new(buffer.clone()))
        .build();

    console.rule(Some("VeryLongTitle"));

    let output = buffer.0.lock().unwrap();
    let text = String::from_utf8_lossy(&output);

    println!("Output: {:?}", text);

    assert!(
        text.trim().len() <= 10,
        "Output '{}' exceeds width 10",
        text.trim()
    );
}
