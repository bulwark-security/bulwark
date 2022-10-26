use crate::traits::*;

pub type NewRootContext = fn(context_id: u32) -> Box<dyn RootContext>;
pub type NewHttpContext = fn(context_id: u32, root_context_id: u32) -> Box<dyn HttpContext>;

#[repr(u32)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum LogLevel {
    Trace = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
    Critical = 5,
}

#[repr(C)]
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct Decision {
    pub accept: f64,
    pub restrict: f64,
    pub unknown: f64,
}

#[repr(u32)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[non_exhaustive]
pub enum Status {
    Ok = 0,
    // The result could not be found, e.g. a provided key did not appear in a
    // table.
    NotFound = 1,
    // An argument was bad, e.g. did not not conform to the required range.
    BadArgument = 2,
    // A protobuf could not be serialized.
    SerializationFailure = 3,
    // A protobuf could not be parsed.
    ParseFailure = 4,
    // A provided expression (e.g. "foo.bar") was illegal or unrecognized.
    BadExpression = 5,
    // A provided memory range was not legal.
    InvalidMemoryAccess = 6,
    // Data was requested from an empty container.
    Empty = 7,
    // The provided CAS did not match that of the stored data.
    CasMismatch = 8,
    // Returned result was unexpected, e.g. of the incorrect size.
    ResultMismatch = 9,
    // Internal failure: trying check logs of the surrounding system.
    InternalFailure = 10,
    // The connection/stream/pipe was broken/closed unexpectedly.
    BrokenConnection = 11,
    // Feature not implemented.
    Unimplemented = 12,
}

#[repr(u32)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[non_exhaustive]
pub enum ContextType {
    HttpContext = 0,
}

#[repr(u32)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[non_exhaustive]
pub enum StreamType {
    HttpRequest = 0,
    HttpResponse = 1,
    Downstream = 2,
    Upstream = 3,
}

#[repr(u32)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[non_exhaustive]
pub enum BufferType {
    HttpRequestBody = 0,
    HttpResponseBody = 1,
    DownstreamData = 2,
    UpstreamData = 3,
    HttpCallResponseBody = 4,
    GrpcReceiveBuffer = 5,
    VmConfiguration = 6,
    PluginConfiguration = 7,
}

#[repr(u32)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[non_exhaustive]
pub enum MapType {
    HttpRequestHeaders = 0,
    HttpRequestTrailers = 1,
    HttpResponseHeaders = 2,
    HttpResponseTrailers = 3,
    GrpcReceiveInitialMetadata = 4,
    GrpcReceiveTrailingMetadata = 5,
    HttpCallResponseHeaders = 6,
    HttpCallResponseTrailers = 7,
}

#[repr(u32)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[non_exhaustive]
pub enum PeerType {
    Unknown = 0,
    Local = 1,
    Remote = 2,
}

#[repr(u32)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[non_exhaustive]
pub enum MetricType {
    Counter = 0,
    Gauge = 1,
    Histogram = 2,
}

pub type Bytes = Vec<u8>;
