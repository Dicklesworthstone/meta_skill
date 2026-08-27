use kasuari::Solver;

#[test]
fn solver_is_send_sync() {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Solver>();
}
