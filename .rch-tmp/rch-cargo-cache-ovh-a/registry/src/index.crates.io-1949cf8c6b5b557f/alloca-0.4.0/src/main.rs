fn main() {
    alloca::with_alloca(128, |_mem| {
        panic!("Hello!");
    });
}
