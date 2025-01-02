use futures::{
    channel::oneshot,
    executor::block_on,
    future::{select, Either, FusedFuture, Shared},
    Future, FutureExt,
};
use std::pin::Pin;
use std::task::{Context, Poll};

/// Future that resolves when the exit signal has fired.
#[derive(Clone)]
pub struct Exit(Shared<oneshot::Receiver<()>>);

impl Future for Exit {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let receiver = &mut Pin::into_inner(self).0;

        if receiver.is_terminated() {
            Poll::Ready(())
        } else {
            Pin::new(receiver).poll(cx).map(drop)
        }
    }
}

impl Exit {
    /// Perform given work until complete.
    pub fn until<F: Future + Unpin>(self, future: F) -> impl Future<Output = Option<F::Output>> {
        select(self, future).map(|either| match either {
            Either::Left(_) => None,
            Either::Right((output, _)) => Some(output),
        })
    }

    /// Block the current thread until complete.
    pub fn wait(self) {
        block_on(self)
    }
}

/// Exit signal that fires either manually or on drop.
pub struct Signal(oneshot::Sender<()>);

impl Signal {
    /// Fire the signal manually.
    pub fn fire(self) -> Result<(), ()> {
        self.0.send(())
    }
}

/// Create a signal and exit pair. `Exit` is a future that resolves when the `Signal` object is
/// either dropped or has `fire` called on it.
pub fn signal() -> (Signal, Exit) {
    let (sender, receiver) = oneshot::channel();
    (Signal(sender), Exit(receiver.shared()))
}
