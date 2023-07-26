use super::*;

use http::{HeaderMap, HeaderValue};
pub(super) use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use std::fmt;

/// Axum state for the admin service.
pub(super) struct AdminState {
    /// State for the health check endpoints
    pub health: HealthState,
    /// State for the metrics endpoint
    pub metrics: MetricsState,
}

/// The health state structure tracks the health of the primary service, primarily for the benefit of
/// external monitoring by load balancers or container orchestration systems.
#[derive(Serialize, Clone, Copy)]
pub(super) struct HealthState {
    /// Indicates that the primary service is live and and has not entered an unrecoverable state.
    ///
    /// In theory, this could become false after the service has started if the system detects that it has
    /// entered a deadlock state, however this is not currently implemented. See
    /// [Kubernete's liveness probes](https://kubernetes.io/docs/tasks/configure-pod-container/configure-liveness-readiness-startup-probes/#define-a-liveness-http-request)
    /// for discussion.
    pub live: bool,
    /// Indicates that the primary service has successfully initialized and is ready to receive requests.
    ///
    /// Once true, it will remain true for the lifetime of the process.
    pub started: bool,
    /// Indicates that the primary service has successfully initialized and is ready to receive requests.
    ///
    /// Once true, it will remain true for the lifetime of the process.
    /// While semantically different from startup state, it's implementation logic is identical.
    pub ready: bool,
}

/// The default probe handler is intended to be at the apex of the health check resource. It simply performs
/// a liveness health check by default.
///
/// See [`probe_handler`].
pub(super) async fn default_probe_handler(
    State(state): State<Arc<Mutex<AdminState>>>,
) -> (StatusCode, Json<HealthState>) {
    probe_handler(State(state), Path(String::from("live"))).await
}

/// The probe handler returns a JSON [`HealthState`] with a status code that depends on the probe type requested.
///
/// - live - Always returns an HTTP OK status if the endpoint is serving requests.
/// - started - Returns an HTTP OK status if the primary service has started and is ready to receive requests
///     and a Service Unavailable status otherwise.
/// - ready - Returns an HTTP OK status if the primary service is available and ready to receive requests
///     and a Service Unavailable status otherwise.
pub(super) async fn probe_handler(
    State(state): State<Arc<Mutex<AdminState>>>,
    Path(probe): Path<String>,
) -> (StatusCode, Json<HealthState>) {
    let state = state.lock().expect("poisoned mutex");
    let status = match probe.as_str() {
        "live" => StatusCode::OK,
        "started" => {
            if state.health.started {
                StatusCode::OK
            } else {
                StatusCode::SERVICE_UNAVAILABLE
            }
        }
        "ready" => {
            if state.health.ready {
                StatusCode::OK
            } else {
                StatusCode::SERVICE_UNAVAILABLE
            }
        }
        // hint that the wrong probe value was sent
        _ => StatusCode::NOT_FOUND,
    };
    (status, Json(state.health))
}

/// The metrics handler is a crawlable endpoint that returns Prometheus metrics.
pub(super) async fn metrics_handler(
    State(state): State<Arc<Mutex<AdminState>>>,
) -> (StatusCode, HeaderMap, Vec<u8>) {
    let state = state.lock().expect("poisoned mutex");
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain"),
    );

    // TODO: Enable process metrics collection. (libproc.h issue, maybe behind cfg feature)
    // (*state.metrics.collect)();

    let metrics = state.metrics.prometheus_handle.render();
    // TODO: Add gzip compression support.
    let body: Vec<u8> = metrics.into();

    (StatusCode::OK, headers, body)
}

/// State for the metrics endpoint
#[derive(Clone)]
pub(super) struct MetricsState {
    prometheus_handle: PrometheusHandle,
    // collect: Arc<dyn Fn() + Send + Sync + 'static>,
}

impl MetricsState {
    /// Creates a new [`MetricsState`] with a Prometheus recorder.
    pub(super) fn new(
        prometheus_handle: PrometheusHandle,
        // collect: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        Self {
            prometheus_handle,
            // collect: Arc::new(collect),
        }
    }
}

impl fmt::Debug for MetricsState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Prometheus")
            .field(
                "prometheus_handle",
                &format_args!("PrometheusHandle {{ ... }}"),
            )
            .finish()
    }
}

/// Checks if the incoming request advertises support for gzip compression.
#[allow(dead_code)]
fn accepts_gzip(headers: &http::header::HeaderMap) -> bool {
    headers
        .get_all(http::header::ACCEPT_ENCODING)
        .iter()
        .any(|value| {
            value
                .to_str()
                .ok()
                .map(|value| value.contains("gzip"))
                .unwrap_or(false)
        })
}
