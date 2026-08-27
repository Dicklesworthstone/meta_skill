use async_lock::Mutex;
use event_listener::Event;

pub struct Pool<T> {
    state: Mutex<(Vec<T>, usize)>,
    ev: Event,
    create_max: usize,
}

impl<T> Pool<T> {
    pub fn new(create_max: usize) -> Self {
        Self { state: Mutex::new((Vec::new(), 0)), ev: Event::new(), create_max }
    }

    pub async fn put(&self, item: T) {
        {
            let mut g = self.state.lock().await;
            g.0.push(item);
        }
        self.ev.notify(1);
    }

    pub async fn take_or_create<F>(&self, create: F) -> T
    where
        F: Fn() -> T,
    {
        loop {
            {
                let mut g = self.state.lock().await;

                if !g.0.is_empty() {
                    return g.0.remove(0);
                }

                if g.1 < self.create_max {
                    g.0.push(create());
                    g.1 += 1;
                    return g.0.remove(0);
                }

                let listener = self.ev.listen();
                drop(g);
                listener.await;  // wake when someone `put`s
            }
        }
    }
}
