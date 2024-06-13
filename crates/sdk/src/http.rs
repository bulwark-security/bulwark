/// Reexports most of the `http` crate.
pub use http::{
    uri, Extensions, HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Uri, Version,
};
pub mod request {
    pub use http::request::{Builder, Parts};
}
pub mod response {
    pub use http::response::{Builder, Parts};
}
/// An HTTP request combines a head consisting of a [`Method`](http::Method), [`Uri`](http::Uri), and headers with [`Bytes`], which provides
/// access to the first chunk of a request body.
pub type Request = http::Request<crate::Bytes>;
/// An HTTP response combines a head consisting of a [`StatusCode`](http::StatusCode) and headers with [`Bytes`], which provides
/// access to the first chunk of a response body.
pub type Response = http::Response<crate::Bytes>;

use crate::errors::HttpSendError;
use crate::wit::wasi::http::outgoing_handler;
#[doc(inline)]
pub use crate::wit::wasi::http::types::{
    ErrorCode, Fields, Headers, IncomingBody, IncomingRequest, IncomingResponse, OutgoingBody,
    OutgoingRequest, OutgoingResponse, Scheme, Trailers,
};
use crate::wit::wasi::io::streams::{InputStream, OutputStream, StreamError};
use async_trait::async_trait;
use futures::SinkExt;
use std::cell::RefCell;
use std::future::Future;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::task::{Poll, Wake, Waker};

const READ_SIZE: u64 = 16 * 1024;

/// A trait for converting a type into an `OutgoingRequest`
trait TryIntoOutgoingRequest {
    /// The error if the conversion fails
    type Error;

    /// Turn the type into an `OutgoingRequest`
    ///
    /// If the implementor can be sure that the `OutgoingRequest::write` has not been called they
    /// can return a buffer as the second element of the returned tuple and `send` will send
    /// that as the request body.
    fn try_into_outgoing_request(self) -> Result<(OutgoingRequest, Option<Vec<u8>>), Self::Error>;
}

impl TryIntoOutgoingRequest for OutgoingRequest {
    type Error = std::convert::Infallible;

    fn try_into_outgoing_request(self) -> Result<(OutgoingRequest, Option<Vec<u8>>), Self::Error> {
        Ok((self, None))
    }
}

impl TryIntoOutgoingRequest for Request {
    type Error = anyhow::Error;

    fn try_into_outgoing_request(self) -> Result<(OutgoingRequest, Option<Vec<u8>>), Self::Error> {
        let (parts, body) = self.into_parts();
        let (method, uri, headers) = (parts.method, parts.uri, parts.headers);
        let is_https = if let Some(scheme) = uri.scheme() {
            scheme == &http::uri::Scheme::HTTPS
        } else {
            false
        };
        let headers = headers
            .iter()
            .map(|(k, v)| (k.to_string(), v.as_bytes().to_vec()))
            .collect::<Vec<_>>();
        let request = OutgoingRequest::new(Headers::from_list(&headers)?);
        request
            .set_method(match method {
                http::Method::GET => &crate::wit::wasi::http::types::Method::Get,
                http::Method::HEAD => &crate::wit::wasi::http::types::Method::Head,
                http::Method::POST => &crate::wit::wasi::http::types::Method::Post,
                http::Method::PUT => &crate::wit::wasi::http::types::Method::Put,
                http::Method::DELETE => &crate::wit::wasi::http::types::Method::Delete,
                http::Method::PATCH => &crate::wit::wasi::http::types::Method::Patch,
                http::Method::CONNECT => &crate::wit::wasi::http::types::Method::Connect,
                http::Method::OPTIONS => &crate::wit::wasi::http::types::Method::Options,
                http::Method::TRACE => &crate::wit::wasi::http::types::Method::Trace,
                _ => return Err(anyhow::anyhow!("unsupported method: {:?}", method)),
            })
            .map_err(|()| anyhow::anyhow!("error setting method to {}", method))?;
        request
            .set_path_with_query(uri.path_and_query().map(|path| path.as_str()))
            .map_err(|()| anyhow::anyhow!("error setting path to {:?}", uri.path_and_query()))?;
        request
            .set_scheme(Some(if is_https {
                &Scheme::Https
            } else {
                &Scheme::Http
            }))
            // According to the documentation, `Request::set_scheme` can only fail due to a malformed
            // `Scheme::Other` payload, but we never pass `Scheme::Other` above, hence the `expect`.
            .expect("unexpected scheme");
        let authority = uri
            .authority()
            .map(|authority| authority.as_str())
            // `wasi-http` requires an authority for outgoing requests, so we always supply one:
            .or(Some(if is_https { ":443" } else { ":80" }));
        request
            .set_authority(authority)
            .map_err(|()| anyhow::anyhow!("error setting authority to {authority:?}"))?;
        Ok((request, Some(body.to_vec())))
    }
}

/// A trait for converting from an `IncomingRequest`
#[async_trait]
trait TryFromIncomingResponse {
    /// The error if conversion fails
    type Error;
    /// Turn the `IncomingResponse` into the type
    async fn try_from_incoming_response(resp: IncomingResponse) -> Result<Self, Self::Error>
    where
        Self: Sized;
}

#[async_trait]
impl TryFromIncomingResponse for IncomingResponse {
    type Error = std::convert::Infallible;
    async fn try_from_incoming_response(resp: IncomingResponse) -> Result<Self, Self::Error> {
        Ok(resp)
    }
}

#[async_trait]
impl TryFromIncomingResponse for Response {
    type Error = HttpSendError;

    async fn try_from_incoming_response(response: IncomingResponse) -> Result<Self, Self::Error> {
        let mut new_resp = http::response::Builder::new()
            .status(response.status())
            .status(response.status());
        for (name, value) in response.headers().entries() {
            new_resp = new_resp.header(name, value);
        }
        let body = bytes::Bytes::from(
            response
                .into_body()
                .await
                .map_err(|e| HttpSendError::GenericIo(e.to_debug_string()))?,
        );
        Ok(new_resp.body(body)?)
    }
}

impl IncomingResponse {
    /// Return a `Stream` from which the body of the specified response may be read.
    ///
    /// # Panics
    ///
    /// Panics if the body was already consumed.
    // TODO: This should ideally take ownership of `self` and be called `into_body_stream` (i.e. symmetric with
    // `IncomingRequest::into_body_stream`).  However, as of this writing, `wasmtime-wasi-http` is implemented in
    // such a way that dropping an `IncomingResponse` will cause the request to be cancelled, meaning the caller
    // won't necessarily have a chance to send the request body if they haven't started doing so yet (or, if they
    // have started, they might not be able to finish before the connection is closed).  See
    // https://github.com/bytecodealliance/wasmtime/issues/7413 for details.
    fn take_body_stream(
        &self,
    ) -> impl futures::Stream<Item = Result<Vec<u8>, crate::wit::wasi::io::streams::Error>> {
        incoming_body(self.consume().expect("response body was already consumed"))
    }

    /// Return a `Vec<u8>` of the body or fails
    ///
    /// # Panics
    ///
    /// Panics if the body was already consumed.
    async fn into_body(self) -> Result<Vec<u8>, crate::wit::wasi::io::streams::Error> {
        use futures::TryStreamExt;
        let mut stream = self.take_body_stream();
        let mut body = Vec::new();
        while let Some(chunk) = stream.try_next().await? {
            body.extend(chunk);
        }
        Ok(body)
    }
}

impl OutgoingResponse {
    /// Construct a `Sink` which writes chunks to the body of the specified response.
    ///
    /// # Panics
    ///
    /// Panics if the body was already taken.
    pub fn take_body(&self) -> impl futures::Sink<Vec<u8>, Error = StreamError> {
        outgoing_body(self.body().expect("response body was already taken"))
    }
}

impl OutgoingRequest {
    /// Construct a `Sink` which writes chunks to the body of the specified response.
    ///
    /// # Panics
    ///
    /// Panics if the body was already taken.
    pub fn take_body(&self) -> impl futures::Sink<Vec<u8>, Error = StreamError> {
        outgoing_body(self.body().expect("request body was already taken"))
    }
}

/// Send an outgoing request
pub fn send(request: Request) -> Result<Response, HttpSendError> {
    run(send_async(request))
}

static WAKERS: Mutex<Vec<(crate::wit::wasi::io::poll::Pollable, Waker)>> = Mutex::new(Vec::new());

fn push_waker(pollable: crate::wit::wasi::io::poll::Pollable, waker: Waker) {
    WAKERS
        .lock()
        .expect("poisoned mutex")
        .push((pollable, waker));
}

/// Run the specified future to completion blocking until it yields a result.
///
/// Based on an executor using `wasi::io/poll/poll-list`,
fn run<T>(future: impl Future<Output = T>) -> T {
    futures::pin_mut!(future);
    struct DummyWaker;

    impl Wake for DummyWaker {
        fn wake(self: Arc<Self>) {}
    }

    let waker = Arc::new(DummyWaker).into();

    loop {
        match future
            .as_mut()
            .poll(&mut std::task::Context::from_waker(&waker))
        {
            Poll::Pending => {
                let mut new_wakers = Vec::new();

                let wakers = std::mem::take::<Vec<_>>(&mut WAKERS.lock().expect("poisoned mutex"));

                assert!(!wakers.is_empty());

                let pollables = wakers
                    .iter()
                    .map(|(pollable, _)| pollable)
                    .collect::<Vec<_>>();

                let mut ready = vec![false; wakers.len()];

                for index in crate::wit::wasi::io::poll::poll(&pollables) {
                    ready[usize::try_from(index).unwrap()] = true;
                }

                for (ready, (pollable, waker)) in ready.into_iter().zip(wakers) {
                    if ready {
                        waker.wake()
                    } else {
                        new_wakers.push((pollable, waker));
                    }
                }

                *WAKERS.lock().expect("poisoned mutex") = new_wakers;
            }
            Poll::Ready(result) => break result,
        }
    }
}

/// Send an outgoing request
async fn send_async<I, O>(request: I) -> Result<O, HttpSendError>
where
    I: TryIntoOutgoingRequest,
    I::Error: Into<Box<dyn std::error::Error + Send + Sync>> + 'static,
    O: TryFromIncomingResponse,
    O::Error: Into<Box<dyn std::error::Error + Send + Sync>> + 'static,
{
    let (request, body_buffer) =
        I::try_into_outgoing_request(request).map_err(|_| HttpSendError::RequestConversion)?;
    let response = if let Some(body_buffer) = body_buffer {
        // It is part of the contract of the trait that implementors of `TryIntoOutgoingRequest`
        // do not call `OutgoingRequest::write`` if they return a buffered body.
        let mut body_sink = request.take_body();
        let response = outgoing_request_send(request);
        body_sink
            .send(body_buffer)
            .await
            .map_err(HttpSendError::StreamIo)?;
        drop(body_sink);
        response.await.map_err(HttpSendError::WasiHttp)?
    } else {
        outgoing_request_send(request)
            .await
            .map_err(HttpSendError::WasiHttp)?
    };

    TryFromIncomingResponse::try_from_incoming_response(response)
        .await
        .map_err(|_| HttpSendError::ResponseConversion)
}

fn outgoing_body(body: OutgoingBody) -> impl futures::Sink<Vec<u8>, Error = StreamError> {
    struct Outgoing(Option<(OutputStream, OutgoingBody)>);

    impl Drop for Outgoing {
        fn drop(&mut self) {
            if let Some((stream, body)) = self.0.take() {
                drop(stream);
                _ = OutgoingBody::finish(body, None);
            }
        }
    }

    let stream = body.write().expect("response body should be writable");
    let pair = Rc::new(RefCell::new(Outgoing(Some((stream, body)))));

    futures::sink::unfold((), {
        move |(), chunk: Vec<u8>| {
            futures::future::poll_fn({
                let mut offset = 0;
                let mut flushing = false;
                let pair = pair.clone();

                move |context| {
                    let pair = pair.borrow();
                    let (stream, _) = &pair.0.as_ref().unwrap();
                    loop {
                        match stream.check_write() {
                            Ok(0) => {
                                push_waker(stream.subscribe(), context.waker().clone());
                                break Poll::Pending;
                            }
                            Ok(count) => {
                                if offset == chunk.len() {
                                    if flushing {
                                        break Poll::Ready(Ok(()));
                                    } else {
                                        match stream.flush() {
                                            Ok(()) => flushing = true,
                                            Err(StreamError::Closed) => break Poll::Ready(Ok(())),
                                            Err(e) => break Poll::Ready(Err(e)),
                                        }
                                    }
                                } else {
                                    let count =
                                        usize::try_from(count).unwrap().min(chunk.len() - offset);

                                    match stream.write(&chunk[offset..][..count]) {
                                        Ok(()) => {
                                            offset += count;
                                        }
                                        Err(e) => break Poll::Ready(Err(e)),
                                    }
                                }
                            }
                            // If the stream is closed but the entire chunk was
                            // written then we've done all we could so this
                            // chunk is now complete.
                            Err(StreamError::Closed) if offset == chunk.len() => {
                                break Poll::Ready(Ok(()))
                            }
                            Err(e) => break Poll::Ready(Err(e)),
                        }
                    }
                }
            })
        }
    })
}

fn incoming_body(
    body: IncomingBody,
) -> impl futures::Stream<Item = Result<Vec<u8>, crate::wit::wasi::io::streams::Error>> {
    struct Incoming(Option<(InputStream, IncomingBody)>);

    impl Drop for Incoming {
        fn drop(&mut self) {
            if let Some((stream, body)) = self.0.take() {
                drop(stream);
                IncomingBody::finish(body);
            }
        }
    }

    futures::stream::poll_fn({
        let stream = body.stream().expect("response body should be readable");
        let pair = Incoming(Some((stream, body)));

        move |context| {
            if let Some((stream, _)) = &pair.0 {
                match stream.read(READ_SIZE) {
                    Ok(buffer) => {
                        if buffer.is_empty() {
                            push_waker(stream.subscribe(), context.waker().clone());
                            Poll::Pending
                        } else {
                            Poll::Ready(Some(Ok(buffer)))
                        }
                    }
                    Err(StreamError::Closed) => Poll::Ready(None),
                    Err(StreamError::LastOperationFailed(error)) => Poll::Ready(Some(Err(error))),
                }
            } else {
                Poll::Ready(None)
            }
        }
    })
}

/// Send the specified request and return the response.
fn outgoing_request_send(
    request: OutgoingRequest,
) -> impl Future<Output = Result<IncomingResponse, ErrorCode>> {
    let response = outgoing_handler::handle(request, None);
    futures::future::poll_fn({
        move |context| match &response {
            Ok(response) => {
                if let Some(response) = response.get() {
                    Poll::Ready(response.unwrap())
                } else {
                    push_waker(response.subscribe(), context.waker().clone());
                    Poll::Pending
                }
            }
            Err(error) => Poll::Ready(Err(error.clone())),
        }
    })
}
