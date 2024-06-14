/// Reexport from the `http` crate.
pub use http::{
    uri, Extensions, HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Uri, Version,
};
/// Reexport from the `http` crate.
pub mod request {
    pub use http::request::{Builder, Parts};
}
/// Reexport from the `http` crate.
pub mod response {
    pub use http::response::{Builder, Parts};
}
/// An HTTP request combines a head consisting of a [`Method`], [`Uri`], and headers with [`Bytes`](crate::Bytes), which provides
/// access to the first chunk of a request body.
pub type Request = http::Request<crate::Bytes>;
/// An HTTP response combines a head consisting of a [`StatusCode`] and headers with [`Bytes`](crate::Bytes), which provides
/// access to the first chunk of a response body.
pub type Response = http::Response<crate::Bytes>;

use crate::wit::wasi::http::outgoing_handler;
use crate::wit::wasi::http::types::{
    ErrorCode, Headers, IncomingBody, IncomingResponse, OutgoingBody, OutgoingRequest, Scheme,
};
use crate::wit::wasi::io::streams::{InputStream, OutputStream, StreamError};
use futures::SinkExt;
use std::cell::RefCell;
use std::future::Future;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::task::{Poll, Wake, Waker};

const READ_SIZE: u64 = 16 * 1024;

/// Returned when there is an error sending an outgoing HTTP request.
#[derive(thiserror::Error, Debug)]
pub enum SendError {
    /// A generic I/O error
    #[error("i/o error: {0}")]
    GenericIo(String),
    /// A stream I/O error
    #[error(transparent)]
    StreamIo(#[from] crate::wit::wasi::io::streams::StreamError),
    /// An HTTP error originating from the WASI HTTP bindings
    #[error(transparent)]
    WasiHttp(#[from] crate::wit::wasi::http::types::ErrorCode),
    /// An HTTP error originating from the HTTP client
    #[error(transparent)]
    ClientHttp(#[from] http::Error),
    /// A request conversion error
    #[error("could not convert request: {0}")]
    RequestConversion(String),
    /// A response conversion error
    #[error("could not convert response")]
    ResponseConversion,
}

/// Send an outgoing request.
///
/// Requires `http` permissions to be set in the plugin configuration for any domain or host this function will be
/// making requests to. In the example below, this would require the `http` permission value to be set to at least
/// `["www.example.com"]`.
///
/// Note that if an HTTP request version is set, it will be ignored because the underlying `wasi-http` API abstracts
/// HTTP version and transport protocol choices.
///
/// # Example
///
#[cfg_attr(doctest, doc = " ````no_test")]
/// ```rust
/// use bulwark_sdk::*;
/// use std::collections::HashMap;
///
/// pub struct HttpExample;
///
/// #[bulwark_plugin]
/// impl HttpHandlers for HttpExample {
///     fn handle_request_decision(
///         _request: http::Request,
///         _labels: HashMap<String, String>,
///     ) -> Result<HandlerOutput, Error> {
///         let _response = http::send(
///             http::request::Builder::new()
///                 .method("GET")
///                 .uri("http://www.example.com/")
///                 .body(Bytes::new())?,
///         )?;
///         // Do something with the response
///
///         Ok(HandlerOutput::default())
///     }
/// }
/// ```
pub fn send(request: Request) -> Result<Response, SendError> {
    run(send_async(request))
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

impl IncomingResponse {
    /// Return a `Stream` from which the body of the specified response may be read.
    ///
    /// # Panics
    ///
    /// Panics if the body was already consumed.
    fn take_body_stream(
        self,
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
async fn send_async(request: Request) -> Result<Response, SendError> {
    // Convert the request into an `OutgoingRequest`.
    let (parts, body) = request.into_parts();
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
    let out_req = OutgoingRequest::new(
        Headers::from_list(&headers)
            .map_err(|e| SendError::RequestConversion(format!("header error: {}", e)))?,
    );
    out_req
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
            _ => {
                return Err(
                    crate::wit::wasi::http::types::ErrorCode::HttpRequestMethodInvalid.into(),
                )
            }
        })
        .map_err(|()| {
            SendError::RequestConversion(format!("could not set method to {}", method))
        })?;
    out_req
        .set_path_with_query(uri.path_and_query().map(|path| path.as_str()))
        .map_err(|()| {
            SendError::RequestConversion(format!(
                "error setting path to {:?}",
                uri.path_and_query()
            ))
        })?;
    out_req
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
    out_req.set_authority(authority).map_err(|()| {
        SendError::RequestConversion(format!("error setting authority to {authority:?}"))
    })?;

    let (request, body_buffer) = (out_req, Some(body.to_vec()));

    let response = if let Some(body_buffer) = body_buffer {
        // It is part of the contract of the trait that implementors of `TryIntoOutgoingRequest`
        // do not call `OutgoingRequest::write`` if they return a buffered body.
        let mut body_sink = request.take_body();
        let response = outgoing_request_send(request);
        body_sink
            .send(body_buffer)
            .await
            .map_err(SendError::StreamIo)?;
        drop(body_sink);
        response.await.map_err(SendError::WasiHttp)?
    } else {
        outgoing_request_send(request)
            .await
            .map_err(SendError::WasiHttp)?
    };

    // Convert the `IncomingResponse` into a `Response`.
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
            .map_err(|e| SendError::GenericIo(e.to_debug_string()))?,
    );
    Ok(new_resp.body(body)?)

    // TryFromIncomingResponse::try_from_incoming_response(response)
    //     .await
    //     .map_err(|_| SendError::ResponseConversion)
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
