//! Running backend work off the UI thread.
//!
//! The UI is immediate-mode: every frame it asks "is that result here yet?"
//! rather than awaiting it. A [`Task`] is the answer to that question — a
//! future spawned on the shared Tokio runtime whose output is picked up by
//! [`Task::poll`] on a later frame. Completion requests a repaint, so a
//! finished task is noticed immediately even when nothing else is moving.

use std::future::Future;
use std::sync::Arc;

use eframe::egui;
use tokio::sync::oneshot;

use crate::state::AppState;

/// Everything a page needs to start backend work.
#[derive(Clone)]
pub struct Ctx {
    pub rt: tokio::runtime::Handle,
    pub egui: egui::Context,
    pub state: Arc<AppState>,
}

impl Ctx {
    /// Spawn a future; the UI collects its output with [`Task::poll`].
    pub fn spawn<T, F>(&self, fut: F) -> Task<T>
    where
        T: Send + 'static,
        F: Future<Output = T> + Send + 'static,
    {
        Task::spawn(&self.rt, &self.egui, fut)
    }

    /// Spawn a closure that needs the backend state.
    pub fn run<T, Fut>(&self, f: impl FnOnce(Arc<AppState>) -> Fut) -> Task<T>
    where
        T: Send + 'static,
        Fut: Future<Output = T> + Send + 'static,
    {
        self.spawn(f(Arc::clone(&self.state)))
    }

    /// Run blocking work (SQLite, config files, the keychain) on the
    /// blocking pool so a slow disk never stalls a frame.
    pub fn blocking<T>(&self, f: impl FnOnce(&AppState) -> T + Send + 'static) -> Task<T>
    where
        T: Send + 'static,
    {
        let state = Arc::clone(&self.state);
        self.spawn(async move {
            tokio::task::spawn_blocking(move || f(&state))
                .await
                .expect("blocking backend task panicked")
        })
    }

    /// Spawn work whose result nobody waits for.
    pub fn detach(&self, fut: impl Future<Output = ()> + Send + 'static) {
        self.rt.spawn(fut);
    }
}

/// The pending output of a spawned future.
pub struct Task<T> {
    rx: oneshot::Receiver<T>,
}

impl<T: Send + 'static> Task<T> {
    pub fn spawn<F>(rt: &tokio::runtime::Handle, ctx: &egui::Context, fut: F) -> Self
    where
        F: Future<Output = T> + Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        let ctx = ctx.clone();
        rt.spawn(async move {
            let _ = tx.send(fut.await);
            ctx.request_repaint();
        });
        Self { rx }
    }

    /// The output, once — `None` while still running.
    ///
    /// A task that panicked never produces output; it is reported by the
    /// Sentry panic handler, and `poll` keeps returning `None`. Callers that
    /// show a spinner should use [`poll_slot`], which clears the slot.
    pub fn poll(&mut self) -> Option<Result<T, ()>> {
        match self.rx.try_recv() {
            Ok(v) => Some(Ok(v)),
            Err(oneshot::error::TryRecvError::Empty) => None,
            Err(oneshot::error::TryRecvError::Closed) => Some(Err(())),
        }
    }
}

/// Poll an optional task in place: returns its output and empties the slot
/// once it finishes. A task that died without output also empties the slot,
/// so a spinner tied to `slot.is_some()` cannot spin forever.
pub fn poll_slot<T: Send + 'static>(slot: &mut Option<Task<T>>) -> Option<T> {
    let out = slot.as_mut()?.poll()?;
    *slot = None;
    out.ok()
}
