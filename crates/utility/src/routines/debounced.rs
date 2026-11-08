use std::{collections::HashMap, future::poll_fn, hash::Hash, sync::Arc, time::Duration};

use tokio::sync::{Semaphore, mpsc::UnboundedReceiver};
use tokio_util::time::{DelayQueue, delay_queue::Key};

pub struct Debounced<T> {
    queue: DelayQueue<T>,
    delay: Duration,
    keys: HashMap<T, Key>,
}

impl<T: Hash + Eq + Clone> Debounced<T> {
    pub fn new(delay: Duration) -> Self {
        Self {
            queue: DelayQueue::new(),
            delay,
            keys: HashMap::new(),
        }
    }

    /// Runs until the sender is dropped, then runs the items still pending once their delay ends.
    pub async fn receive_and_process<F, Fut>(self, mut f: F, recv: UnboundedReceiver<T>)
    where
        F: FnMut(T) -> Fut,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.run(recv, |item| {
            tokio::spawn(f(item));
        })
        .await;
    }

    /// Like `receive_and_process`, but runs `f` on tokio's blocking pool, for CPU-heavy or blocking work,
    /// one job per CPU at most.
    pub async fn receive_and_process_blocking<F>(self, f: F, recv: UnboundedReceiver<T>)
    where
        F: Fn(T) + Send + Sync + 'static,
        T: Send + 'static,
    {
        let n = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(2);
        self.receive_and_process_blocking_concurrent(f, recv, n).await;
    }

    /// Like `receive_and_process_blocking`, with at most `in_flight` jobs (at least 1) running at once.
    pub async fn receive_and_process_blocking_concurrent<F>(
        self,
        f: F,
        recv: UnboundedReceiver<T>,
        in_flight: usize,
    ) where
        F: Fn(T) + Send + Sync + 'static,
        T: Send + 'static,
    {
        let f = Arc::new(f);
        let permits = Arc::new(Semaphore::new(in_flight.max(1)));
        self.run(recv, move |item| {
            let f = Arc::clone(&f);
            let permits = Arc::clone(&permits);
            // Waiting for a permit in a task, not in the loop, keeps the debounce running meanwhile.
            tokio::spawn(async move {
                let permit = permits.acquire_owned().await.expect("never closed");
                tokio::task::spawn_blocking(move || {
                    let _permit = permit;
                    f(item)
                });
            });
        })
        .await;
    }

    async fn run(mut self, mut recv: UnboundedReceiver<T>, mut spawn: impl FnMut(T)) {
        loop {
            tokio::select! {
                item = recv.recv() => match item {
                    Some(item) => self.push(item),
                    None => break,
                },
                // An empty DelayQueue yields None at once, so only poll it when it has entries.
                Some(expired) = poll_fn(|cx| self.queue.poll_expired(cx)), if !self.queue.is_empty() => {
                    let item = expired.into_inner();
                    self.keys.remove(&item);
                    spawn(item);
                }
            }
        }

        // Drain all the items after the recv closes.
        while let Some(expired) = poll_fn(|cx| self.queue.poll_expired(cx)).await {
            spawn(expired.into_inner());
        }
    }

    fn push(&mut self, item: T) {
        match self.keys.get(&item) {
            Some(key) => self.queue.reset(key, self.delay),
            None => {
                let key = self.queue.insert(item.clone(), self.delay);
                self.keys.insert(item, key);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tokio::sync::mpsc;

    use super::*;
    use crate::timeutil::{Instant, sleep};

    const DELAY: Duration = Duration::from_secs(5);

    enum Step {
        Push(&'static str),
        Wait(u64),
    }
    use Step::*;

    /// Plays `steps` into a debouncer, closes the channel, and returns each item with the second it ran at.
    async fn play(steps: &[Step], blocking: bool) -> Vec<(u64, &'static str)> {
        let start = Instant::now();
        let (tx, rx) = mpsc::unbounded_channel();
        let (out_tx, mut out_rx) = mpsc::unbounded_channel();
        let record = move |item: &'static str| {
            out_tx.send((start.elapsed().as_secs(), item)).unwrap();
        };

        let debounced = Debounced::new(DELAY);
        let task = if blocking {
            tokio::spawn(debounced.receive_and_process_blocking(record, rx))
        } else {
            let f = move |item| {
                let record = record.clone();
                async move { record(item) }
            };
            tokio::spawn(debounced.receive_and_process(f, rx))
        };

        for step in steps {
            match step {
                Push(item) => tx.send(*item).unwrap(),
                Wait(secs) => sleep(Duration::from_secs(*secs)).await,
            }
        }
        drop(tx);
        task.await.unwrap();

        // Ends once every job has finished and dropped its sender.
        let mut ran = Vec::new();
        while let Some(run) = out_rx.recv().await {
            ran.push(run);
        }
        ran.sort();
        ran
    }

    #[tokio::test(start_paused = true)]
    async fn runs_item_after_delay() {
        let ran = play(&[Push("a"), Wait(10)], false).await;
        assert_eq!(ran, [(5, "a")]);
    }

    #[tokio::test(start_paused = true)]
    async fn repeat_resets_timer() {
        let ran = play(&[Push("a"), Wait(3), Push("a"), Wait(10)], false).await;
        assert_eq!(ran, [(8, "a")]);
    }

    #[tokio::test(start_paused = true)]
    async fn distinct_items_have_own_timers() {
        let ran = play(&[Push("a"), Wait(1), Push("b"), Wait(10)], false).await;
        assert_eq!(ran, [(5, "a"), (6, "b")]);
    }

    #[tokio::test(start_paused = true)]
    async fn item_can_run_again_after_firing() {
        let ran = play(&[Push("a"), Wait(6), Push("a"), Wait(10)], false).await;
        assert_eq!(ran, [(5, "a"), (11, "a")]);
    }

    #[tokio::test(start_paused = true)]
    async fn pending_items_run_after_close() {
        let ran = play(&[Push("a"), Push("b"), Wait(2), Push("a")], false).await;
        assert_eq!(ran, [(5, "b"), (7, "a")]);
    }

    #[tokio::test(start_paused = true)]
    async fn close_with_nothing_pending_returns() {
        let ran = play(&[], false).await;
        assert!(ran.is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn blocking_debounces_like_async() {
        let ran = play(&[Push("a"), Push("b"), Wait(1), Push("a"), Wait(10)], true).await;
        assert_eq!(ran, [(5, "b"), (6, "a")]);
    }

    #[tokio::test(start_paused = true)]
    async fn blocking_runs_pending_items_after_close() {
        let ran = play(&[Push("a"), Wait(2), Push("a")], true).await;
        assert_eq!(ran, [(7, "a")]);
    }

    #[tokio::test(start_paused = true)]
    async fn blocking_caps_jobs_in_flight() {
        let (tx, rx) = mpsc::unbounded_channel();
        for i in 0..6 {
            tx.send(i).unwrap();
        }
        drop(tx);

        let max = Arc::new(AtomicUsize::new(0));
        let (done_tx, mut done_rx) = mpsc::unbounded_channel();
        let job = {
            let now = AtomicUsize::new(0);
            let max = max.clone();
            move |_: i32| {
                let running = now.fetch_add(1, Ordering::SeqCst) + 1;
                max.fetch_max(running, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(20));
                now.fetch_sub(1, Ordering::SeqCst);
                done_tx.send(()).unwrap();
            }
        };
        Debounced::new(DELAY)
            .receive_and_process_blocking_concurrent(job, rx, 2)
            .await;

        let mut runs = 0;
        while done_rx.recv().await.is_some() {
            runs += 1;
        }
        assert_eq!(runs, 6);
        assert_eq!(max.load(Ordering::SeqCst), 2);
    }
}
