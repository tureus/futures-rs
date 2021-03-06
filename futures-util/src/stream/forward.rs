use crate::stream::{StreamExt, Fuse};
use core::marker::Unpin;
use core::mem::PinMut;
use futures_core::future::Future;
use futures_core::stream::Stream;
use futures_core::task::{self, Poll};
use futures_sink::Sink;
use pin_utils::{unsafe_pinned, unsafe_unpinned};

const INVALID_POLL: &str = "polled `Forward` after completion";

/// Future for the `Stream::forward` combinator, which sends a stream of values
/// to a sink and then flushes the sink.
///
/// Note: this is only usable with `Unpin` sinks, so `Sink`s that aren't `Unpin`
/// will need to be pinned in order to be used with this combinator.
//
// This limitation is necessary in order to return the sink after the forwarding
// has completed so that it can be used again.
#[derive(Debug)]
#[must_use = "steams do nothing unless polled"]
pub struct Forward<St: Stream, Si: Sink + Unpin> {
    sink: Option<Si>,
    stream: Fuse<St>,
    buffered_item: Option<Si::SinkItem>,
}

impl<St: Stream + Unpin, Si: Sink + Unpin> Unpin for Forward<St, Si> {}

impl<St, Si> Forward<St, Si>
where
    Si: Sink + Unpin,
    St: Stream<Item = Result<Si::SinkItem, Si::SinkError>>,
{
    unsafe_pinned!(sink: Option<Si>);
    unsafe_pinned!(stream: Fuse<St>);
    unsafe_unpinned!(buffered_item: Option<Si::SinkItem>);

    pub(super) fn new(stream: St, sink: Si) -> Forward<St, Si> {
    Forward {
        sink: Some(sink),
        stream: stream.fuse(),
            buffered_item: None,
    }
}

    fn try_start_send(
        mut self: PinMut<Self>,
        cx: &mut task::Context,
        item: Si::SinkItem,
    ) -> Poll<Result<(), Si::SinkError>> {
        debug_assert!(self.buffered_item.is_none());
        {
            let mut sink = self.sink().as_pin_mut().unwrap();
            if try_poll!(sink.reborrow().poll_ready(cx)).is_ready() {
                return Poll::Ready(sink.start_send(item));
            }
        }
        *self.buffered_item() = Some(item);
        Poll::Pending
    }
}

impl<St, Si> Future for Forward<St, Si>
where
    Si: Sink + Unpin,
    St: Stream<Item = Result<Si::SinkItem, Si::SinkError>>,
{
    type Output = Result<Si, Si::SinkError>;

    fn poll(
        mut self: PinMut<Self>,
        cx: &mut task::Context,
    ) -> Poll<Self::Output> {
        // If we've got an item buffered already, we need to write it to the
        // sink before we can do anything else
        if let Some(item) = self.buffered_item().take() {
            try_ready!(self.reborrow().try_start_send(cx, item));
        }

        loop {
            match self.stream().poll_next(cx) {
                Poll::Ready(Some(Ok(item))) =>
                   try_ready!(self.reborrow().try_start_send(cx, item)),
                Poll::Ready(Some(Err(e))) => return Poll::Ready(Err(e)),
                Poll::Ready(None) => {
                    try_ready!(self.sink().as_pin_mut().expect(INVALID_POLL)
                                   .poll_close(cx));
                    return Poll::Ready(Ok(self.sink().take().unwrap()))
                }
                Poll::Pending => {
                    try_ready!(self.sink().as_pin_mut().expect(INVALID_POLL)
                                   .poll_flush(cx));
                    return Poll::Pending
                }
            }
        }
    }
}
