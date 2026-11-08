use anyhow::Result;
use tokio::task::{JoinError, JoinSet};

pub async fn for_each<I, F, Fut>(items: &[I], f: F)
where
    F: FnMut(usize, &I) -> Fut,
    Fut: Future<Output = ()> + Send + 'static,
{
    let n = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2);
    for_each_concurrent(items, n, f).await;
}

pub async fn for_each_concurrent<I, F, Fut>(items: &[I], in_flight: usize, mut f: F)
where
    F: FnMut(usize, &I) -> Fut,
    Fut: Future<Output = ()> + Send + 'static,
{
    transform_concurrent_unordered(items, in_flight, |i, item| {
        let fut = f(i, item);
        async move {
            fut.await;
            anyhow::Ok(())
        }
    })
    .await;
}

pub async fn transform<I, F, O, Fut>(items: &[I], f: F) -> Vec<Result<O>>
where
    F: FnMut(usize, &I) -> Fut,
    Fut: Future<Output = Result<O>> + Send + 'static,
    O: Send + 'static,
{
    let n = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2);
    transform_concurrent(items, n, f).await
}

pub async fn transform_concurrent<I, F, O, Fut>(
    items: &[I],
    in_flight: usize,
    mut f: F,
) -> Vec<Result<O>>
where
    F: FnMut(usize, &I) -> Fut,
    Fut: Future<Output = Result<O>> + Send + 'static,
    O: Send + 'static,
{
    let mut tagged = transform_concurrent_unordered(items, in_flight, |i, item| {
        let fut = f(i, item); // borrow of f and item ends here
        async move { fut.await.map(|o| (i, o)).map_err(|e| (i, e)) }
    })
    .await;
    tagged.sort_by_key(|r| match r {
        Ok((i, _)) | Err((i, _)) => *i,
    });
    tagged
        .into_iter()
        .map(|r| r.map(|(_, o)| o).map_err(|(_, e)| e))
        .collect()
}

pub async fn transform_concurrent_unordered<I, F, O, E, Fut>(
    items: &[I],
    in_flight: usize,
    mut f: F,
) -> Vec<Result<O, E>>
where
    F: FnMut(usize, &I) -> Fut,
    Fut: Future<Output = Result<O, E>> + Send + 'static,
    O: Send + 'static,
    E: Send + 'static,
{
    let in_flight = in_flight.max(1);
    let mut set = JoinSet::new();
    let mut result: Vec<Result<O, E>> = Vec::with_capacity(items.len());

    for (i, item) in items.into_iter().enumerate() {
        if set.len() >= in_flight
            && let Some(j) = set.join_next().await
        {
            result.push(flatten_result(j));
        }
        let fut = f(i, item);
        set.spawn(async move { fut.await });
    }
    while let Some(j) = set.join_next().await {
        let p = flatten_result(j);
        result.push(p);
    }
    result
}

pub async fn for_each_blocking<I, F, Job>(items: &[I], f: F)
where
    F: FnMut(usize, &I) -> Job,
    Job: FnOnce() + Send + 'static,
{
    let n = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2);
    for_each_concurrent_blocking(items, n, f).await;
}

pub async fn for_each_concurrent_blocking<I, F, Job>(items: &[I], in_flight: usize, mut f: F)
where
    F: FnMut(usize, &I) -> Job,
    Job: FnOnce() + Send + 'static,
{
    transform_concurrent_unordered_blocking(items, in_flight, |i, item| {
        let job = f(i, item);
        move || {
            job();
            anyhow::Ok(())
        }
    })
    .await;
}

pub async fn transform_blocking<I, F, O, Job>(items: &[I], f: F) -> Vec<Result<O>>
where
    F: FnMut(usize, &I) -> Job,
    Job: FnOnce() -> Result<O> + Send + 'static,
    O: Send + 'static,
{
    let n = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2);
    transform_concurrent_blocking(items, n, f).await
}

pub async fn transform_concurrent_blocking<I, F, O, Job>(
    items: &[I],
    in_flight: usize,
    mut f: F,
) -> Vec<Result<O>>
where
    F: FnMut(usize, &I) -> Job,
    Job: FnOnce() -> Result<O> + Send + 'static,
    O: Send + 'static,
{
    let mut tagged = transform_concurrent_unordered_blocking(items, in_flight, |i, item| {
        let job = f(i, item);
        move || job().map(|o| (i, o)).map_err(|e| (i, e))
    })
    .await;
    tagged.sort_by_key(|r| match r {
        Ok((i, _)) | Err((i, _)) => *i,
    });
    tagged
        .into_iter()
        .map(|r| r.map(|(_, o)| o).map_err(|(_, e)| e))
        .collect()
}

pub async fn transform_concurrent_unordered_blocking<I, F, O, E, Job>(
    items: &[I],
    in_flight: usize,
    mut f: F,
) -> Vec<Result<O, E>>
where
    F: FnMut(usize, &I) -> Job,
    Job: FnOnce() -> Result<O, E> + Send + 'static,
    O: Send + 'static,
    E: Send + 'static,
{
    let in_flight = in_flight.max(1);
    let mut set = JoinSet::new();
    let mut result: Vec<Result<O, E>> = Vec::with_capacity(items.len());

    for (i, item) in items.iter().enumerate() {
        if set.len() >= in_flight
            && let Some(j) = set.join_next().await
        {
            result.push(flatten_result(j));
        }
        set.spawn_blocking(f(i, item));
    }
    while let Some(j) = set.join_next().await {
        let p = flatten_result(j);
        result.push(p);
    }
    result
}

fn flatten_result<T, E>(res: Result<Result<T, E>, JoinError>) -> Result<T, E> {
    match res {
        Ok(Ok(t)) => Ok(t),
        Err(e) => std::panic::resume_unwind(e.into_panic()), // this should never happen
        Ok(Err(e)) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    use anyhow::anyhow;

    use super::*;
    use crate::timeutil::{Duration, sleep};

    /// Counts how many jobs run at once, the most seen, and how many ran in total.
    #[derive(Clone, Default)]
    struct Gauge {
        now: Arc<AtomicUsize>,
        max: Arc<AtomicUsize>,
        runs: Arc<AtomicUsize>,
    }

    impl Gauge {
        fn enter(&self) {
            let now = self.now.fetch_add(1, Ordering::SeqCst) + 1;
            self.max.fetch_max(now, Ordering::SeqCst);
            self.runs.fetch_add(1, Ordering::SeqCst);
        }

        fn exit(&self) {
            self.now.fetch_sub(1, Ordering::SeqCst);
        }

        fn max(&self) -> usize {
            self.max.load(Ordering::SeqCst)
        }

        fn runs(&self) -> usize {
            self.runs.load(Ordering::SeqCst)
        }
    }

    fn values<O>(results: Vec<Result<O>>) -> Vec<O> {
        results.into_iter().map(|r| r.unwrap()).collect()
    }

    fn shown<O: ToString>(results: &[Result<O>]) -> Vec<String> {
        results
            .iter()
            .map(|r| match r {
                Ok(o) => o.to_string(),
                Err(e) => e.to_string(),
            })
            .collect()
    }

    async fn gauge_async(items: usize, in_flight: usize) -> Gauge {
        let gauge = Gauge::default();
        for_each_concurrent(&vec![(); items], in_flight, |_, _| {
            let gauge = gauge.clone();
            async move {
                gauge.enter();
                sleep(Duration::from_millis(10)).await;
                gauge.exit();
            }
        })
        .await;
        gauge
    }

    #[tokio::test]
    async fn transform_concurrent_keeps_input_order() {
        let delays = [30u64, 20, 10, 0];
        let results = transform_concurrent(&delays, delays.len(), |_, &ms| async move {
            sleep(Duration::from_millis(ms)).await;
            Ok(ms)
        })
        .await;
        assert_eq!(values(results), delays);
    }

    #[tokio::test]
    async fn transform_concurrent_unordered_returns_in_completion_order() {
        let delays = [30u64, 20, 10, 0];
        let results = transform_concurrent_unordered(&delays, delays.len(), |_, &ms| async move {
            sleep(Duration::from_millis(ms)).await;
            anyhow::Ok(ms)
        })
        .await;
        assert_eq!(values(results), [0, 10, 20, 30]);
    }

    #[tokio::test]
    async fn in_flight_caps_running_tasks() {
        let gauge = gauge_async(10, 3).await;
        assert_eq!(gauge.max(), 3);
        assert_eq!(gauge.runs(), 10);
    }

    #[tokio::test]
    async fn zero_in_flight_runs_one_at_a_time() {
        let gauge = gauge_async(5, 0).await;
        assert_eq!(gauge.max(), 1);
        assert_eq!(gauge.runs(), 5);
    }

    #[tokio::test]
    async fn f_gets_each_item_with_its_index() {
        let items: Vec<usize> = (0..20).map(|i| i * 10).collect();
        let results = transform(&items, |i, &item| async move { Ok((i, item)) }).await;
        let expected: Vec<_> = (0..20).map(|i| (i, i * 10)).collect();
        assert_eq!(values(results), expected);
    }

    #[tokio::test]
    async fn errors_keep_their_position() {
        let items: Vec<u32> = (0..6).collect();
        let results = transform_concurrent(&items, 2, |_, &n| async move {
            if n % 2 == 0 {
                Ok(n)
            } else {
                Err(anyhow!("odd {n}"))
            }
        })
        .await;
        assert_eq!(shown(&results), ["0", "odd 1", "2", "odd 3", "4", "odd 5"]);
    }

    #[tokio::test]
    #[should_panic(expected = "boom")]
    async fn a_panicking_task_panics_the_caller() {
        for_each(&[1, 2, 3], |_, &n| async move {
            if n == 2 {
                panic!("boom");
            }
        })
        .await;
    }

    #[tokio::test]
    async fn transform_concurrent_blocking_keeps_input_order() {
        let delays = [30u64, 20, 10, 0];
        let results = transform_concurrent_blocking(&delays, delays.len(), |_, &ms| {
            move || {
                std::thread::sleep(Duration::from_millis(ms));
                if ms == 20 { Err(anyhow!("slow")) } else { Ok(ms) }
            }
        })
        .await;
        assert_eq!(shown(&results), ["30", "slow", "10", "0"]);
    }

    #[tokio::test]
    async fn blocking_in_flight_caps_running_jobs() {
        let gauge = Gauge::default();
        for_each_concurrent_blocking(&[(); 8], 2, |_, _| {
            let gauge = gauge.clone();
            move || {
                gauge.enter();
                std::thread::sleep(Duration::from_millis(20));
                gauge.exit();
            }
        })
        .await;
        assert_eq!(gauge.max(), 2);
        assert_eq!(gauge.runs(), 8);
    }

    #[tokio::test]
    async fn for_each_blocking_visits_every_item_once() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let items: Vec<usize> = (0..20).collect();
        for_each_blocking(&items, |i, &item| {
            let seen = seen.clone();
            move || {
                seen.lock().unwrap().push((i, item));
            }
        })
        .await;
        let mut seen = std::mem::take(&mut *seen.lock().unwrap());
        seen.sort();
        let expected: Vec<_> = (0..20).map(|i| (i, i)).collect();
        assert_eq!(seen, expected);
    }

    #[tokio::test]
    async fn blocking_empty_input_gives_empty_output() {
        let items: [u32; 0] = [];
        let results = transform_blocking(&items, |_, &n| move || Ok(n)).await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    #[should_panic(expected = "boom")]
    async fn a_panicking_job_panics_the_caller() {
        for_each_blocking(&[1, 2, 3], |_, &n| {
            move || {
                if n == 2 {
                    panic!("boom");
                }
            }
        })
        .await;
    }
}
