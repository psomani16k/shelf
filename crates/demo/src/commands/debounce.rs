use std::time::Duration;

use shelf_utility::routines::Debounced;
use tokio::{
    sync::mpsc::unbounded_channel,
    time::{Instant, sleep},
};

command! {
    /// Replays a scripted burst of file events through `Debounced`. With
    /// `blocking`, jobs run on the blocking pool.
    pub async fn debounce(&self, delay: u64, blocking: bool) {
        let events:Vec<(&str, u64)> = vec![
            ("a.epub", 1),
            ("b.epub", 1),
            ("a.epub", 1),
            ("b.epub", 1),
            ("a.epub", 3),
            ("b.epub", 3),
        ];
        let (tx, rx) = unbounded_channel();
        let debouncer: Debounced<&str> = Debounced::new(Duration::from_secs(delay));
        let j = tokio::spawn(
        debouncer.receive_and_process(async |book|{
            println!("processing {} now", book);
        }, rx));

        let instant = Instant::now();
        for (file, event_delay) in events {
            println!("Sending {} at {:?}", file, instant.elapsed());
            tx.send(file).unwrap();
            sleep(Duration::from_secs(event_delay)).await;
        }

        tx.closed();
        j.await;

        println!()
    }
}
