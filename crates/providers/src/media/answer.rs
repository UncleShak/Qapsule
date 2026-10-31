//! Players can hang. Every asynchronous call to one gets a deadline here, and
//! a button press runs on its own thread, so a stuck player never stops the
//! provider from following the others. Windows-free, so it is tested with fakes.

use std::sync::mpsc::{SyncSender, sync_channel};
use std::time::Duration;

/// How long a player has to answer one call.
pub const ANSWER_WITHIN: Duration = Duration::from_secs(3);

/// An asynchronous call in flight.
pub trait Op {
    type Output;
    /// Runs `done` once the call has finished (at once if it already has).
    /// `Err` carries an error code.
    fn on_done(&self, done: Box<dyn FnOnce() + Send>) -> Result<(), i32>;
    fn result(&self) -> Result<Self::Output, i32>;
    /// Asks the call to stop, where its interface allows it.
    fn cancel(&self);
}

#[derive(Debug, PartialEq, Eq)]
pub enum Waited<T> {
    Done(T),
    TimedOut,
    /// The call failed, with this error code.
    Failed(i32),
}

/// Waits for `op` for at most `within`. On a timeout the call is cancelled and left behind.
pub fn wait<O: Op>(op: &O, within: Duration) -> Waited<O::Output> {
    let (tx, rx) = sync_channel::<()>(1);
    if let Err(code) = op.on_done(Box::new(move || {
        let _ = tx.try_send(());
    })) {
        return Waited::Failed(code);
    }
    match rx.recv_timeout(within) {
        Ok(()) => match op.result() {
            Ok(value) => Waited::Done(value),
            Err(code) => Waited::Failed(code),
        },
        Err(_) => {
            op.cancel();
            Waited::TimedOut
        }
    }
}

/// Runs one job at a time on its own thread, with one more waiting. When both
/// places are taken (a hung player), further jobs are refused at once.
pub struct Background<T> {
    tx: SyncSender<T>,
}

impl<T: Send + 'static> Background<T> {
    pub fn spawn(name: &str, mut job: impl FnMut(T) + Send + 'static) -> Option<Self> {
        let (tx, rx) = sync_channel::<T>(1);
        std::thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                for item in rx {
                    job(item);
                }
            })
            .ok()?;
        Some(Self { tx })
    }

    /// Hands `item` over without waiting. `false` if the thread is busy and its place is taken.
    pub fn offer(&self, item: T) -> bool {
        self.tx.try_send(item).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    use super::*;

    /// A call that never finishes.
    #[derive(Default)]
    struct Hangs {
        cancelled: AtomicBool,
    }

    impl Op for Hangs {
        type Output = u8;
        fn on_done(&self, _done: Box<dyn FnOnce() + Send>) -> Result<(), i32> {
            Ok(())
        }
        fn result(&self) -> Result<u8, i32> {
            panic!("a call that hasn't finished has no result");
        }
        fn cancel(&self) {
            self.cancelled.store(true, Ordering::SeqCst);
        }
    }

    /// A call that has finished by the time anyone asks.
    struct Finished(Result<u8, i32>);

    impl Op for Finished {
        type Output = u8;
        fn on_done(&self, done: Box<dyn FnOnce() + Send>) -> Result<(), i32> {
            done();
            Ok(())
        }
        fn result(&self) -> Result<u8, i32> {
            self.0
        }
        fn cancel(&self) {
            panic!("a finished call is not cancelled");
        }
    }

    #[test]
    fn a_call_that_hangs_times_out_and_is_cancelled() {
        let op = Hangs::default();
        let started = Instant::now();
        assert_eq!(wait(&op, Duration::from_millis(30)), Waited::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(op.cancelled.load(Ordering::SeqCst));
    }

    #[test]
    fn finished_calls_return_their_value_or_their_error() {
        assert_eq!(wait(&Finished(Ok(7)), ANSWER_WITHIN), Waited::Done(7));
        assert_eq!(wait(&Finished(Err(-5)), ANSWER_WITHIN), Waited::Failed(-5));
    }

    #[test]
    fn a_hung_button_never_blocks_the_caller_and_a_full_thread_refuses() {
        let release = Arc::new(Mutex::new(()));
        let held = release.lock().unwrap();
        let gate = release.clone();
        let ran = Arc::new(AtomicBool::new(false));
        let seen = ran.clone();
        let background = Background::spawn("test-background", move |_: u8| {
            // Hangs until the test lets go.
            drop(gate.lock().unwrap());
            seen.store(true, Ordering::SeqCst);
        })
        .unwrap();

        let started = Instant::now();
        let mut accepted = 0;
        for i in 0..50 {
            accepted += usize::from(background.offer(i));
        }
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "offers never wait"
        );
        assert!(
            (1..=2).contains(&accepted),
            "one running, at most one waiting; {accepted} accepted"
        );
        drop(held);
        let deadline = Instant::now() + Duration::from_secs(2);
        while !ran.load(Ordering::SeqCst) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(ran.load(Ordering::SeqCst));
    }
}
