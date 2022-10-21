use crate::hostcalls;
use crate::types::*;
use std::time::{Duration, SystemTime};

pub trait Context {
    fn get_current_time(&self) -> SystemTime {
        hostcalls::get_current_time().unwrap()
    }

    fn get_property(&self, path: Vec<&str>) -> Option<Bytes> {
        hostcalls::get_property(path).unwrap()
    }

    fn set_property(&self, path: Vec<&str>, value: Option<&[u8]>) {
        hostcalls::set_property(path, value).unwrap()
    }

    fn get_shared_data(&self, key: &str) -> (Option<Bytes>, Option<u32>) {
        hostcalls::get_shared_data(key).unwrap()
    }

    fn set_shared_data(
        &self,
        key: &str,
        value: Option<&[u8]>,
        cas: Option<u32>,
    ) -> Result<(), Status> {
        hostcalls::set_shared_data(key, value, cas)
    }

    fn register_shared_queue(&self, name: &str) -> u32 {
        hostcalls::register_shared_queue(name).unwrap()
    }

    fn resolve_shared_queue(&self, vm_id: &str, name: &str) -> Option<u32> {
        hostcalls::resolve_shared_queue(vm_id, name).unwrap()
    }

    fn dequeue_shared_queue(&self, queue_id: u32) -> Result<Option<Bytes>, Status> {
        hostcalls::dequeue_shared_queue(queue_id)
    }

    fn enqueue_shared_queue(&self, queue_id: u32, value: Option<&[u8]>) -> Result<(), Status> {
        hostcalls::enqueue_shared_queue(queue_id, value)
    }

    fn dispatch_http_call(
        &self,
        upstream: &str,
        headers: Vec<(&str, &str)>,
        body: Option<&[u8]>,
        trailers: Vec<(&str, &str)>,
        timeout: Duration,
    ) -> Result<u32, Status> {
        hostcalls::dispatch_http_call(upstream, headers, body, trailers, timeout)
    }

    fn on_http_call_response(
        &mut self,
        _token_id: u32,
        _num_headers: usize,
        _body_size: usize,
        _num_trailers: usize,
    ) {
    }

    fn get_http_call_response_headers(&self) -> Vec<(String, String)> {
        hostcalls::get_map(MapType::HttpCallResponseHeaders).unwrap()
    }

    fn get_http_call_response_headers_bytes(&self) -> Vec<(String, Bytes)> {
        hostcalls::get_map_bytes(MapType::HttpCallResponseHeaders).unwrap()
    }

    fn get_http_call_response_header(&self, name: &str) -> Option<String> {
        hostcalls::get_map_value(MapType::HttpCallResponseHeaders, name).unwrap()
    }

    fn get_http_call_response_header_bytes(&self, name: &str) -> Option<Bytes> {
        hostcalls::get_map_value_bytes(MapType::HttpCallResponseHeaders, name).unwrap()
    }

    fn get_http_call_response_body(&self, start: usize, max_size: usize) -> Option<Bytes> {
        hostcalls::get_buffer(BufferType::HttpCallResponseBody, start, max_size).unwrap()
    }

    fn get_http_call_response_trailers(&self) -> Vec<(String, String)> {
        hostcalls::get_map(MapType::HttpCallResponseTrailers).unwrap()
    }

    fn get_http_call_response_trailers_bytes(&self) -> Vec<(String, Bytes)> {
        hostcalls::get_map_bytes(MapType::HttpCallResponseTrailers).unwrap()
    }

    fn get_http_call_response_trailer(&self, name: &str) -> Option<String> {
        hostcalls::get_map_value(MapType::HttpCallResponseTrailers, name).unwrap()
    }

    fn get_http_call_response_trailer_bytes(&self, name: &str) -> Option<Bytes> {
        hostcalls::get_map_value_bytes(MapType::HttpCallResponseTrailers, name).unwrap()
    }

    fn dispatch_grpc_call(
        &self,
        upstream_name: &str,
        service_name: &str,
        method_name: &str,
        initial_metadata: Vec<(&str, &[u8])>,
        message: Option<&[u8]>,
        timeout: Duration,
    ) -> Result<u32, Status> {
        hostcalls::dispatch_grpc_call(
            upstream_name,
            service_name,
            method_name,
            initial_metadata,
            message,
            timeout,
        )
    }

    fn on_grpc_call_response(&mut self, _token_id: u32, _status_code: u32, _response_size: usize) {}

    fn get_grpc_call_response_body(&self, start: usize, max_size: usize) -> Option<Bytes> {
        hostcalls::get_buffer(BufferType::GrpcReceiveBuffer, start, max_size).unwrap()
    }

    fn cancel_grpc_call(&self, token_id: u32) {
        hostcalls::cancel_grpc_call(token_id).unwrap()
    }

    fn open_grpc_stream(
        &self,
        cluster_name: &str,
        service_name: &str,
        method_name: &str,
        initial_metadata: Vec<(&str, &[u8])>,
    ) -> Result<u32, Status> {
        hostcalls::open_grpc_stream(cluster_name, service_name, method_name, initial_metadata)
    }

    fn on_grpc_stream_initial_metadata(&mut self, _token_id: u32, _num_elements: u32) {}

    fn get_grpc_stream_initial_metadata(&self) -> Vec<(String, Bytes)> {
        hostcalls::get_map_bytes(MapType::GrpcReceiveInitialMetadata).unwrap()
    }

    fn get_grpc_stream_initial_metadata_value(&self, name: &str) -> Option<Bytes> {
        hostcalls::get_map_value_bytes(MapType::GrpcReceiveInitialMetadata, name).unwrap()
    }

    fn send_grpc_stream_message(&self, token_id: u32, message: Option<&[u8]>, end_stream: bool) {
        hostcalls::send_grpc_stream_message(token_id, message, end_stream).unwrap()
    }

    fn on_grpc_stream_message(&mut self, _token_id: u32, _message_size: usize) {}

    fn get_grpc_stream_message(&mut self, start: usize, max_size: usize) -> Option<Bytes> {
        hostcalls::get_buffer(BufferType::GrpcReceiveBuffer, start, max_size).unwrap()
    }

    fn on_grpc_stream_trailing_metadata(&mut self, _token_id: u32, _num_elements: u32) {}

    fn get_grpc_stream_trailing_metadata(&self) -> Vec<(String, Bytes)> {
        hostcalls::get_map_bytes(MapType::GrpcReceiveTrailingMetadata).unwrap()
    }

    fn get_grpc_stream_trailing_metadata_value(&self, name: &str) -> Option<Bytes> {
        hostcalls::get_map_value_bytes(MapType::GrpcReceiveTrailingMetadata, name).unwrap()
    }

    fn cancel_grpc_stream(&self, token_id: u32) {
        hostcalls::cancel_grpc_stream(token_id).unwrap()
    }

    fn close_grpc_stream(&self, token_id: u32) {
        hostcalls::close_grpc_stream(token_id).unwrap()
    }

    fn on_grpc_stream_close(&mut self, _token_id: u32, _status_code: u32) {}

    fn get_grpc_status(&self) -> (u32, Option<String>) {
        hostcalls::get_grpc_status().unwrap()
    }

    fn on_done(&mut self) -> bool {
        true
    }

    fn done(&self) {
        hostcalls::done().unwrap()
    }
}

pub trait RootContext: Context {
    fn on_vm_start(&mut self, _vm_configuration_size: usize) -> bool {
        true
    }

    fn get_vm_configuration(&self) -> Option<Bytes> {
        hostcalls::get_buffer(BufferType::VmConfiguration, 0, usize::MAX).unwrap()
    }

    fn on_configure(&mut self, _plugin_configuration_size: usize) -> bool {
        true
    }

    fn get_plugin_configuration(&self) -> Option<Bytes> {
        hostcalls::get_buffer(BufferType::PluginConfiguration, 0, usize::MAX).unwrap()
    }

    fn set_tick_period(&self, period: Duration) {
        hostcalls::set_tick_period(period).unwrap()
    }

    fn on_tick(&mut self) {}

    fn on_queue_ready(&mut self, _queue_id: u32) {}

    fn on_log(&mut self) {}

    fn create_http_context(&self, _context_id: u32) -> Option<Box<dyn HttpContext>> {
        None
    }

    fn get_type(&self) -> Option<ContextType> {
        None
    }
}

pub trait HttpContext: Context {
    fn get_http_request_headers(&self) -> Vec<(String, String)> {
        hostcalls::get_map(MapType::HttpRequestHeaders).unwrap()
    }

    fn get_http_request_headers_bytes(&self) -> Vec<(String, Bytes)> {
        hostcalls::get_map_bytes(MapType::HttpRequestHeaders).unwrap()
    }

    fn set_http_request_headers(&self, headers: Vec<(&str, &str)>) {
        hostcalls::set_map(MapType::HttpRequestHeaders, headers).unwrap()
    }

    fn set_http_request_headers_bytes(&self, headers: Vec<(&str, &[u8])>) {
        hostcalls::set_map_bytes(MapType::HttpRequestHeaders, headers).unwrap()
    }

    fn get_http_request_header(&self, name: &str) -> Option<String> {
        hostcalls::get_map_value(MapType::HttpRequestHeaders, name).unwrap()
    }

    fn get_http_request_header_bytes(&self, name: &str) -> Option<Bytes> {
        hostcalls::get_map_value_bytes(MapType::HttpRequestHeaders, name).unwrap()
    }

    fn set_http_request_header(&self, name: &str, value: Option<&str>) {
        hostcalls::set_map_value(MapType::HttpRequestHeaders, name, value).unwrap()
    }

    fn set_http_request_header_bytes(&self, name: &str, value: Option<&[u8]>) {
        hostcalls::set_map_value_bytes(MapType::HttpRequestHeaders, name, value).unwrap()
    }

    fn add_http_request_header(&self, name: &str, value: &str) {
        hostcalls::add_map_value(MapType::HttpRequestHeaders, name, value).unwrap()
    }

    fn add_http_request_header_bytes(&self, name: &str, value: &[u8]) {
        hostcalls::add_map_value_bytes(MapType::HttpRequestHeaders, name, value).unwrap()
    }

    fn on_http_request_initial(
        &mut self,
        _num_headers: usize,
        _readable_body_size: usize,
        _end_of_stream: bool,
    ) {
    }

    fn on_http_request_decision(
        &mut self,
        _num_headers: usize,
        _readable_body_size: usize,
        _end_of_stream: bool,
    ) -> Decision {
        Decision {
            accept: 0.0,
            restrict: 0.0,
            unknown: 1.0,
        }
    }

    fn get_http_request_body(&self, start: usize, max_size: usize) -> Option<Bytes> {
        hostcalls::get_buffer(BufferType::HttpRequestBody, start, max_size).unwrap()
    }

    fn set_http_request_body(&self, start: usize, size: usize, value: &[u8]) {
        hostcalls::set_buffer(BufferType::HttpRequestBody, start, size, value).unwrap()
    }

    fn on_http_request_trailers(&mut self, _num_trailers: usize) {}

    fn get_http_request_trailers(&self) -> Vec<(String, String)> {
        hostcalls::get_map(MapType::HttpRequestTrailers).unwrap()
    }

    fn get_http_request_trailers_bytes(&self) -> Vec<(String, Bytes)> {
        hostcalls::get_map_bytes(MapType::HttpRequestTrailers).unwrap()
    }

    fn set_http_request_trailers(&self, trailers: Vec<(&str, &str)>) {
        hostcalls::set_map(MapType::HttpRequestTrailers, trailers).unwrap()
    }

    fn set_http_request_trailers_bytes(&self, trailers: Vec<(&str, &[u8])>) {
        hostcalls::set_map_bytes(MapType::HttpRequestTrailers, trailers).unwrap()
    }

    fn get_http_request_trailer(&self, name: &str) -> Option<String> {
        hostcalls::get_map_value(MapType::HttpRequestTrailers, name).unwrap()
    }

    fn get_http_request_trailer_bytes(&self, name: &str) -> Option<Bytes> {
        hostcalls::get_map_value_bytes(MapType::HttpRequestTrailers, name).unwrap()
    }

    fn set_http_request_trailer(&self, name: &str, value: Option<&str>) {
        hostcalls::set_map_value(MapType::HttpRequestTrailers, name, value).unwrap()
    }

    fn set_http_request_trailer_bytes(&self, name: &str, value: Option<&[u8]>) {
        hostcalls::set_map_value_bytes(MapType::HttpRequestTrailers, name, value).unwrap()
    }

    fn add_http_request_trailer(&self, name: &str, value: &str) {
        hostcalls::add_map_value(MapType::HttpRequestTrailers, name, value).unwrap()
    }

    fn add_http_request_trailer_bytes(&self, name: &str, value: &[u8]) {
        hostcalls::add_map_value_bytes(MapType::HttpRequestTrailers, name, value).unwrap()
    }

    fn on_http_response_initial(
        &mut self,
        _num_headers: usize,
        _readable_body_size: usize,
        _end_of_stream: bool,
    ) {
    }

    fn on_http_response_decision(
        &mut self,
        _num_headers: usize,
        _readable_body_size: usize,
        _end_of_stream: bool,
    ) -> Decision {
        Decision {
            accept: 0.0,
            restrict: 0.0,
            unknown: 1.0,
        }
    }

    fn get_http_response_headers(&self) -> Vec<(String, String)> {
        hostcalls::get_map(MapType::HttpResponseHeaders).unwrap()
    }

    fn get_http_response_headers_bytes(&self) -> Vec<(String, Bytes)> {
        hostcalls::get_map_bytes(MapType::HttpResponseHeaders).unwrap()
    }

    fn set_http_response_headers(&self, headers: Vec<(&str, &str)>) {
        hostcalls::set_map(MapType::HttpResponseHeaders, headers).unwrap()
    }

    fn set_http_response_headers_bytes(&self, headers: Vec<(&str, &[u8])>) {
        hostcalls::set_map_bytes(MapType::HttpResponseHeaders, headers).unwrap()
    }

    fn get_http_response_header(&self, name: &str) -> Option<String> {
        hostcalls::get_map_value(MapType::HttpResponseHeaders, name).unwrap()
    }

    fn get_http_response_header_bytes(&self, name: &str) -> Option<Bytes> {
        hostcalls::get_map_value_bytes(MapType::HttpResponseHeaders, name).unwrap()
    }

    fn set_http_response_header(&self, name: &str, value: Option<&str>) {
        hostcalls::set_map_value(MapType::HttpResponseHeaders, name, value).unwrap()
    }

    fn set_http_response_header_bytes(&self, name: &str, value: Option<&[u8]>) {
        hostcalls::set_map_value_bytes(MapType::HttpResponseHeaders, name, value).unwrap()
    }

    fn add_http_response_header(&self, name: &str, value: &str) {
        hostcalls::add_map_value(MapType::HttpResponseHeaders, name, value).unwrap()
    }

    fn add_http_response_header_bytes(&self, name: &str, value: &[u8]) {
        hostcalls::add_map_value_bytes(MapType::HttpResponseHeaders, name, value).unwrap()
    }

    fn get_http_response_body(&self, start: usize, max_size: usize) -> Option<Bytes> {
        hostcalls::get_buffer(BufferType::HttpResponseBody, start, max_size).unwrap()
    }

    fn set_http_response_body(&self, start: usize, size: usize, value: &[u8]) {
        hostcalls::set_buffer(BufferType::HttpResponseBody, start, size, value).unwrap()
    }

    fn on_http_response_trailers(&mut self, _num_trailers: usize) {}

    fn get_http_response_trailers(&self) -> Vec<(String, String)> {
        hostcalls::get_map(MapType::HttpResponseTrailers).unwrap()
    }

    fn get_http_response_trailers_bytes(&self) -> Vec<(String, Bytes)> {
        hostcalls::get_map_bytes(MapType::HttpResponseTrailers).unwrap()
    }

    fn set_http_response_trailers(&self, trailers: Vec<(&str, &str)>) {
        hostcalls::set_map(MapType::HttpResponseTrailers, trailers).unwrap()
    }

    fn set_http_response_trailers_bytes(&self, trailers: Vec<(&str, &[u8])>) {
        hostcalls::set_map_bytes(MapType::HttpResponseTrailers, trailers).unwrap()
    }

    fn get_http_response_trailer(&self, name: &str) -> Option<String> {
        hostcalls::get_map_value(MapType::HttpResponseTrailers, name).unwrap()
    }

    fn get_http_response_trailer_bytes(&self, name: &str) -> Option<Bytes> {
        hostcalls::get_map_value_bytes(MapType::HttpResponseTrailers, name).unwrap()
    }

    fn set_http_response_trailer(&self, name: &str, value: Option<&str>) {
        hostcalls::set_map_value(MapType::HttpResponseTrailers, name, value).unwrap()
    }

    fn set_http_response_trailer_bytes(&self, name: &str, value: Option<&[u8]>) {
        hostcalls::set_map_value_bytes(MapType::HttpResponseTrailers, name, value).unwrap()
    }

    fn add_http_response_trailer(&self, name: &str, value: &str) {
        hostcalls::add_map_value(MapType::HttpResponseTrailers, name, value).unwrap()
    }

    fn add_http_response_trailer_bytes(&self, name: &str, value: &[u8]) {
        hostcalls::add_map_value_bytes(MapType::HttpResponseTrailers, name, value).unwrap()
    }

    fn on_log(&mut self) {}
}
