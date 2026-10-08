//! Plays a tournament's games on worker threads, streaming their results.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::tournament::{GameResult, Tournament};

/// The tournament's games, played on `threads` worker threads in
/// [`Tournament::scheduled`] order, each result sent as its game ends.
/// Stopping or dropping it lets the games in progress finish, and plays no
/// more.
pub struct Runner {
    pub results: mpsc::Receiver<GameResult>,
    cancel: Arc<AtomicBool>,
    pause: Arc<AtomicBool>,
    active: Arc<AtomicU32>,
    worker: Option<JoinHandle<()>>,
}

impl Runner {
    pub fn start(tournament: Tournament, threads: usize) -> Self {
        let (tx, results) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let pause = Arc::new(AtomicBool::new(false));
        let active = Arc::new(AtomicU32::new(0));
        let worker = {
            let (cancel, pause, active) = (cancel.clone(), pause.clone(), active.clone());
            thread::spawn(move || {
                let next = AtomicUsize::new(0);
                thread::scope(|scope| {
                    for _ in 0..threads.max(1) {
                        let tx = tx.clone();
                        let (tournament, next) = (&tournament, &next);
                        let (cancel, pause, active) = (&cancel, &pause, &active);
                        scope.spawn(move || loop {
                            while pause.load(Ordering::Relaxed) && !cancel.load(Ordering::Relaxed) {
                                thread::sleep(Duration::from_millis(50));
                            }
                            if cancel.load(Ordering::Relaxed) {
                                return;
                            }
                            let turn = next.fetch_add(1, Ordering::Relaxed);
                            if turn >= tournament.games() {
                                return;
                            }
                            active.fetch_add(1, Ordering::Relaxed);
                            let result = tournament.play(tournament.scheduled(turn));
                            active.fetch_sub(1, Ordering::Relaxed);
                            if tx.send(result).is_err() {
                                return;
                            }
                        });
                    }
                });
            })
        };
        Self {
            results,
            cancel,
            pause,
            active,
            worker: Some(worker),
        }
    }

    /// How many games are being played right now.
    pub fn in_flight(&self) -> u32 {
        self.active.load(Ordering::Relaxed)
    }

    pub fn pause(&self) {
        self.pause.store(true, Ordering::Relaxed);
    }

    pub fn resume(&self) {
        self.pause.store(false, Ordering::Relaxed);
    }

    /// Play no more games, and wait for those in progress to end.
    pub fn stop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            worker.join().ok();
        }
    }
}

impl Drop for Runner {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The number of threads to play on by default: one per core.
pub fn default_threads() -> usize {
    thread::available_parallelism().map_or(1, |n| n.get())
}
