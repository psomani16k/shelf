use shelf_utility::{
    routines::Debounced,
    timeutil::{Duration, Instant, sleep},
};
use tokio::sync::mpsc::unbounded_channel;

command! {
    /// Replays a scripted burst of file events through `Debounced`.
    pub async fn debounce(&self, delay: u64) {
        let events: Vec<(&str, u64)> = vec![
            ("a.epub", 1),
            ("b.epub", 1),
            ("a.epub", 1),
            ("b.epub", 1),
            ("a.epub", 3),
            ("b.epub", 3),
        ];
        let (tx, rx) = unbounded_channel();
        let debouncer: Debounced<&str> = Debounced::new(Duration::from_secs(delay));
        let j = tokio::spawn(debouncer.receive_and_process(
            async |book| println!("processing {book} now"),
            rx,
        ));

        let instant = Instant::now();
        for (file, event_delay) in events {
            println!("Sending {} at {:?}", file, instant.elapsed());
            tx.send(file).unwrap();
            sleep(Duration::from_secs(event_delay)).await;
        }

        // Closing the channel ends `receive_and_process` once the waiting paths have run.
        drop(tx);
        j.await.unwrap();

        println!()
    }
}
