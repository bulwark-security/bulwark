//! Partial implementation of the [Elastic Common Schema (ECS)](https://www.elastic.co/guide/en/ecs/current/ecs-reference.html) for the [`tracing_forest::Formatter`].
//!
//! Bulwark's concurrent execution of plugins would otherwise make some logging presentations unintelligible,
//! and [`tracing_forest`] exists to remedy that. The default [`Pretty`](tracing_forest::printer::Pretty)
//! formatter is a multi-line logger that is easy to visually scan and excellent for local debugging. However
//! it is unsuitable for production logging use-cases where logs could be interleaved from multiple instances.
//! This formatter eliminates that problem by condensing multiple events and spans into a single structured JSON
//! ECS event that typically maps one-to-one with incoming requests. This produces a new-line-delimited JSON log.
//!
//! This module does not currently attempt to implement the entirety of ECS as not all ECS fields are generated
//! by Bulwark and most of the implementation would go unused. The module's non-reusable implementation further
//! limits the value of a complete implementation.
//!
//! The ECS format also allows Bulwark's logs to be cross-referenced with logs from other services that are
//! either emitted as ECS or normalized to it.

use {
    chrono::{DateTime, Utc},
    quoted_string::spec::{PartialCodePoint, QuotingClass},
    serde::{Deserialize, Serialize},
    serde_json::{Map, Value},
    std::fmt,
    tracing_forest::tree::{Event, Span, Tree},
    tracing_forest::Formatter,
};

/// The ECS formatter converts a [`Tree`](tracing_forest::tree::Tree) into a formatted [`String`] to be displayed.
///
/// The formatter expects log messages to be short, known messages emitted by Bulwark and therefore it is
/// not a reusable component.
///
/// An [`EcsFormatter`] is used with a [`ForestLayer`](tracing_forest::ForestLayer) and a [`Printer`](tracing_forest::Printer).
/// It is composed by a [`Registry`](tracing_subscriber::Registry) into a [`Subscriber`](tracing_core::subscriber::Subscriber).
///
/// # Example
///
/// This would print log messages in ECS format to stdout.
///
/// ```
/// let layer = tracing_forest::ForestLayer::from(
///     tracing_forest::Printer::new().formatter(EcsFormatter),
/// )
/// ```
#[derive(Debug)]
pub struct EcsFormatter;

impl Formatter for EcsFormatter {
    type Error = fmt::Error;

    /// Parse a [`tracing_forest::tree::Tree`] and format it as a [`String`] for display.
    fn fmt(&self, tree: &Tree) -> Result<String, fmt::Error> {
        let mut ecs_event = EcsEvent::default();
        EcsFormatter::parse_tree(tree, None, &mut ecs_event)?;
        // We parse the level here because multiple points in the tree have different levels and we want to record the root's level only.
        // If we recorded it within `parse_tree`, the leaf nodes would override the root during recursive calls or we'd have to check if
        // the level was already set for each event or span.
        EcsFormatter::parse_level(tree, &mut ecs_event);
        serde_json::to_string(&ecs_event)
            // The newline must be present to flush the writer and to produce valid newline-delimited JSON
            .map(|json| json.trim().to_string() + "\n")
            .map_err(|_| fmt::Error)
    }
}

impl EcsFormatter {
    fn parse_tree(
        tree: &Tree,
        duration_root: Option<f64>,
        ecs_event: &mut EcsEvent,
    ) -> fmt::Result {
        match tree {
            Tree::Event(event) => EcsFormatter::parse_event(event, ecs_event),
            Tree::Span(span) => EcsFormatter::parse_span(span, duration_root, ecs_event),
        }
    }

    fn parse_level(tree: &Tree, ecs_event: &mut EcsEvent) {
        let level = match tree {
            Tree::Event(event) => event.level(),
            Tree::Span(span) => span.level(),
        };
        let mut log = ecs_event.log.clone().unwrap_or_default();
        log.level = match level {
            tracing::Level::ERROR => String::from("error"),
            tracing::Level::WARN => String::from("warn"),
            tracing::Level::INFO => String::from("info"),
            tracing::Level::DEBUG => String::from("debug"),
            tracing::Level::TRACE => String::from("trace"),
        };
        ecs_event.log = Some(log);
    }

    fn parse_event(event: &Event, ecs_event: &mut EcsEvent) -> fmt::Result {
        ecs_event.timestamp = event.timestamp();

        if let Some(message) = event.message() {
            // Remove double quotes if they're present. We can't be confident that they will be.
            let unquoted_message_result = quoted_string::to_content::<TraceQuoteSpec>(message);
            let message_string: String;
            let message: &str = if let Ok(unquoted_message) = unquoted_message_result {
                message_string = unquoted_message.to_string();
                message_string.as_str()
            } else {
                message
            };
            ecs_event.message = message.to_string();

            match message {
                "load plugin" => EcsFormatter::parse_load_plugin_event(event, ecs_event),
                "process request" => EcsFormatter::parse_process_request_event(event, ecs_event),
                "process response" => EcsFormatter::parse_process_response_event(event, ecs_event),
                "plugin decision" => EcsFormatter::parse_plugin_decision_event(event, ecs_event),
                "combine decision" => EcsFormatter::parse_combine_decision_event(event, ecs_event),
                _ => EcsFormatter::parse_unknown_event(event, ecs_event),
            }
        } else {
            EcsFormatter::parse_unknown_event(event, ecs_event)
        }
    }

    /// Parses `"load plugin"` messages emitted when a plugin is first loaded into the WASM VM.
    fn parse_load_plugin_event(event: &Event, ecs_event: &mut EcsEvent) -> fmt::Result {
        for field in event.fields().iter() {
            match field.key() {
                "path" => {
                    let mut file = ecs_event.file.clone().unwrap_or_default();
                    let unquoted_path = quoted_string::to_content::<TraceQuoteSpec>(field.value())
                        .map_err(|_| fmt::Error)?;
                    file.path = unquoted_path.to_string();
                    ecs_event.file = Some(file);
                }
                "resource" => {
                    let mut url = ecs_event.url.clone().unwrap_or_default();
                    let unquoted_path = quoted_string::to_content::<TraceQuoteSpec>(field.value())
                        .map_err(|_| fmt::Error)?;
                    url.original = unquoted_path.to_string();
                    ecs_event.url = Some(url);
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// Parses `"process request"` messages emitted whenever a request is received.
    fn parse_process_request_event(event: &Event, ecs_event: &mut EcsEvent) -> fmt::Result {
        // TODO: probably worth having some control over verbosity
        let mut event_meta = ecs_event.event.clone().unwrap_or_default();
        event_meta.kind = Some(String::from("event"));
        event_meta.category = Some(vec![String::from("web")]);
        ecs_event.event = Some(event_meta);

        // TODO: figure out if event.uuid() will be our trace.transaction.id?
        // TODO: related: setup up OpenTelemetry / OTLP

        for field in event.fields().iter() {
            match field.key() {
                "method" => {
                    let mut http = ecs_event.http.clone().unwrap_or_default();
                    let mut request = http.request.unwrap_or_default();
                    let unquoted_method =
                        quoted_string::to_content::<TraceQuoteSpec>(field.value())
                            .map_err(|_| fmt::Error)?;
                    request.method = unquoted_method.to_string();
                    http.request = Some(request);
                    ecs_event.http = Some(http);
                }
                "uri" => {
                    let mut url = ecs_event.url.clone().unwrap_or_default();
                    let unquoted_url = quoted_string::to_content::<TraceQuoteSpec>(field.value())
                        .map_err(|_| fmt::Error)?;
                    url.original = unquoted_url.to_string();
                    ecs_event.url = Some(url);
                }
                "user_agent" => {
                    let mut user_agent = ecs_event.user_agent.clone().unwrap_or_default();
                    let unquoted_ua = quoted_string::to_content::<TraceQuoteSpec>(field.value())
                        .map_err(|_| fmt::Error)?;
                    user_agent.original = unquoted_ua.to_string();
                    ecs_event.user_agent = Some(user_agent);
                }
                _ => {}
            }
        }

        let http = ecs_event.http.clone().unwrap_or_default();
        let request = http.request.unwrap_or_default();
        let uri = ecs_event.url.clone().unwrap_or_default();
        let response = http.response.unwrap_or_default();
        let message = format!("{} {} [{}]", request.method, uri.original, response.status);
        ecs_event.message = message;

        Ok(())
    }

    /// Parses `"process response"` messages emitted whenever a response is received.
    fn parse_process_response_event(event: &Event, ecs_event: &mut EcsEvent) -> fmt::Result {
        // TODO: probably worth having some control over verbosity
        let mut event_meta = ecs_event.event.clone().unwrap_or_default();
        event_meta.kind = Some(String::from("event"));
        event_meta.category = Some(vec![String::from("web")]);
        ecs_event.event = Some(event_meta);

        for field in event.fields().iter() {
            if field.key() == "status" {
                let mut http = ecs_event.http.clone().unwrap_or_default();
                let mut response = http.response.unwrap_or_default();
                response.status = str::parse::<i64>(field.value()).map_err(|_| fmt::Error)?;
                http.response = Some(response);
                ecs_event.http = Some(http);
            }
        }

        let http = ecs_event.http.clone().unwrap_or_default();
        let request = http.request.unwrap_or_default();
        let uri = ecs_event.url.clone().unwrap_or_default();
        let response = http.response.unwrap_or_default();
        let message = format!("{} {} [{}]", request.method, uri.original, response.status);
        ecs_event.message = message;

        Ok(())
    }

    /// Parses `"plugin decision"` messages emitted after each request and response phase.
    fn parse_plugin_decision_event(event: &Event, ecs_event: &mut EcsEvent) -> fmt::Result {
        let mut plugin_field_set = EcsBulwarkDecisionFieldSet::default();
        let mut reference_name: Option<String> = None;

        for field in event.fields().iter() {
            match field.key() {
                "name" => {
                    let unquoted_reference =
                        quoted_string::to_content::<TraceQuoteSpec>(field.value())
                            .map_err(|_| fmt::Error)?;
                    reference_name = Some(unquoted_reference.to_string());
                }
                "accept" => {
                    plugin_field_set.accept =
                        str::parse::<f64>(field.value()).map_err(|_| fmt::Error)?;
                }
                "restrict" => {
                    plugin_field_set.restrict =
                        str::parse::<f64>(field.value()).map_err(|_| fmt::Error)?;
                }
                "unknown" => {
                    plugin_field_set.unknown =
                        str::parse::<f64>(field.value()).map_err(|_| fmt::Error)?;
                }
                "score" => {
                    plugin_field_set.score =
                        str::parse::<f64>(field.value()).map_err(|_| fmt::Error)?;
                }
                _ => {}
            }
        }

        // These messages may be sent up to twice for each plugin, once for the request phase and once for
        // the response phase. If there is a message for both phases, the response phase information will
        // overwrite the previous request phase information. This may ultimately prove to be a problematic
        // design. It's optimizing for verbosity and total log transfer, but it's lossy by discarding values
        // that are superceded by decisions in later phases. Since decisions values from earlier phases are
        // carried to the next phase, it should still always report the decision values that contributed to
        // the final decision.
        if let Some(reference_name) = reference_name {
            let mut bulwark = ecs_event.bulwark.clone().unwrap_or_default();
            let mut plugins = bulwark.plugins.clone().unwrap_or_default();
            plugins.insert(
                reference_name,
                serde_json::to_value(plugin_field_set).map_err(|_| fmt::Error)?,
            );
            bulwark.plugins = Some(plugins);
            ecs_event.bulwark = Some(bulwark);
        } else {
            return Err(fmt::Error);
        }

        Ok(())
    }

    /// Parses `"combine decision"` messages emitted for request and response phases which combine individual
    /// plugin decisions into an ensemble decision.
    ///
    /// The decision outcome based on the configured decision thresholds and the combined set of all tags emitted
    /// by the plugins is also parsed from these messages.
    fn parse_combine_decision_event(event: &Event, ecs_event: &mut EcsEvent) -> fmt::Result {
        // If the service is in observe-only mode, don't report events as denied
        // Fields aren't sorted, scanning because we can't binary search
        let observe_only = event
            .fields()
            .iter()
            .find_map(|f| {
                if f.key() == "observe_only" {
                    Some(f.value() == "true")
                } else {
                    None
                }
            })
            .unwrap_or_default();

        for field in event.fields().iter() {
            match field.key() {
                "accept" => {}
                "restrict" => {}
                "unknown" => {}
                "score" => {
                    let mut risk = ecs_event.risk.clone().unwrap_or_default();
                    risk.calculated_risk_score =
                        field.value().parse::<f64>().map_err(|_| fmt::Error)?;
                    ecs_event.risk = Some(risk);
                }
                "outcome" => {
                    let mut risk = ecs_event.risk.clone().unwrap_or_default();
                    let unquoted_outcome =
                        quoted_string::to_content::<TraceQuoteSpec>(field.value())
                            .map_err(|_| fmt::Error)?;
                    risk.calculated_level = unquoted_outcome.to_ascii_lowercase();
                    ecs_event.risk = Some(risk);

                    // TODO: in the future it may be possible for user-defined handling of outcome and may require changes here
                    let mut event_meta = ecs_event.event.clone().unwrap_or_default();
                    if unquoted_outcome.to_ascii_lowercase() == "restricted" && !observe_only {
                        event_meta.type_ = Some(vec![String::from("denied")]);
                    } else {
                        event_meta.type_ = Some(vec![String::from("allowed")]);
                    }
                    ecs_event.event = Some(event_meta);
                }
                "tags" => {
                    let unquoted_tags = quoted_string::to_content::<TraceQuoteSpec>(field.value())
                        .map_err(|_| fmt::Error)?;
                    let tags: Vec<String> = unquoted_tags
                        .to_string()
                        .split(',')
                        .map(|s| s.trim().to_ascii_lowercase())
                        .filter(|s| !s.is_empty())
                        .collect();
                    if !tags.is_empty() {
                        ecs_event.tags = Some(tags);
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// Parses unrecognized messages on a "best effort" basis. Not currently implemented.
    fn parse_unknown_event(event: &Event, _ecs_event: &mut EcsEvent) -> fmt::Result {
        for _field in event.fields().iter() {
            // TODO: make best guesses? right now these should rarely happen at info and debug level and user can switch to forest if needed
        }
        Ok(())
    }

    /// Parses a [`Span`] into an [`EcsEvent`]. Barely implemented.
    fn parse_span(
        span: &Span,
        duration_root: Option<f64>,
        ecs_event: &mut EcsEvent,
    ) -> fmt::Result {
        let total_duration = span.total_duration().as_nanos() as f64;
        let root_duration = duration_root.unwrap_or(total_duration);

        for tree in span.nodes() {
            EcsFormatter::parse_tree(tree, Some(root_duration), ecs_event)?;
        }

        Ok(())
    }
}

/// Quote spec for parsing the quoted strings emitted as field values by tracing.
#[derive(Copy, Clone, Debug)]
struct TraceQuoteSpec;

impl quoted_string::spec::GeneralQSSpec for TraceQuoteSpec {
    type Quoting = Self;
    type Parsing = TraceParsingImpl;
}

impl quoted_string::spec::QuotingClassifier for TraceQuoteSpec {
    fn classify_for_quoting(pcp: PartialCodePoint) -> QuotingClass {
        if !is_valid_pcp(pcp) {
            QuotingClass::Invalid
        } else {
            match pcp.as_u8() {
                b'"' | b'\\' => QuotingClass::NeedsQuoting,
                _ => QuotingClass::QText,
            }
        }
    }
}

fn is_valid_pcp(pcp: PartialCodePoint) -> bool {
    let bch = pcp.as_u8();
    (b' '..=b'~').contains(&bch)
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
struct TraceParsingImpl;

impl quoted_string::spec::ParsingImpl for TraceParsingImpl {
    fn can_be_quoted(pcp: PartialCodePoint) -> bool {
        is_valid_pcp(pcp)
    }
    fn handle_normal_state(
        pcp: PartialCodePoint,
    ) -> Result<(quoted_string::spec::State<Self>, bool), quoted_string::error::CoreError> {
        if is_valid_pcp(pcp) {
            Ok((quoted_string::spec::State::Normal, true))
        } else {
            Err(quoted_string::error::CoreError::InvalidChar)
        }
    }

    fn advance(
        &self,
        pcp: PartialCodePoint,
    ) -> Result<(quoted_string::spec::State<Self>, bool), quoted_string::error::CoreError> {
        if is_valid_pcp(pcp) {
            Ok((quoted_string::spec::State::Normal, false))
        } else {
            Err(quoted_string::error::CoreError::InvalidChar)
        }
    }
}

#[derive(Serialize, Deserialize, Default, Debug)]
pub(crate) struct EcsEvent {
    /// Date/time when the event originated, which will be formatted as a string in RFC3339 format.
    #[serde(
        default = "default_timestamp_now",
        rename(serialize = "@timestamp", deserialize = "@timestamp")
    )]
    timestamp: DateTime<Utc>,
    /// The primary message to be logged.
    message: String,
    /// Details about the event’s logging mechanism or logging transport.
    #[serde(skip_serializing_if = "Option::is_none")]
    log: Option<EcsLogFieldSet>,
    /// Fields related to HTTP activity.
    ///
    /// Use the url field set to store the url of the request.
    #[serde(skip_serializing_if = "Option::is_none")]
    http: Option<EcsHttpFieldSet>,
    /// URL fields provide support for complete or partial URLs.
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<EcsUrlFieldSet>,
    /// The user agent field values originate from browsers and other HTTP clients.
    #[serde(skip_serializing_if = "Option::is_none")]
    user_agent: Option<EcsUserAgentFieldSet>,
    /// A list of keywords used to tag each event.
    #[serde(skip_serializing_if = "Option::is_none")]
    tags: Option<Vec<String>>,
    /// Custom key/value pairs associated with the event.
    ///
    /// While the type definition allows other JSON value types, however only string values should be present.
    #[serde(skip_serializing_if = "Option::is_none")]
    labels: Option<Map<String, Value>>,
    /// The event fields are used for context information about the log or metric event itself.
    #[serde(skip_serializing_if = "Option::is_none")]
    event: Option<EcsEventFieldSet>,
    /// The risk field values represent the system's calculated assessment of risk.
    #[serde(skip_serializing_if = "Option::is_none")]
    risk: Option<EcsRiskFieldSet>,
    /// File fields provide details about the affected file associated with the event or metric.
    #[serde(skip_serializing_if = "Option::is_none")]
    file: Option<EcsFileFieldSet>,
    /// The custom field values namespaced for Bulwark.
    #[serde(skip_serializing_if = "Option::is_none")]
    bulwark: Option<EcsBulwarkFieldSet>,
}

fn default_timestamp_now() -> DateTime<Utc> {
    Utc::now()
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub(crate) struct EcsEventFieldSet {
    /// Kind represents high-level information about what type of information the event contains.
    /// It is the highest-level event categorization in ECS.
    ///
    /// # Allowed values
    ///
    /// - alert
    /// - enrichment
    /// - event
    /// - metric
    /// - state
    /// - pipeline_error
    /// - signal
    #[serde(skip_serializing_if = "Option::is_none")]
    kind: Option<String>,
    /// Category represents a wider range of classification under ECS. Notably it is an array to
    /// allow for events that may be categorized under multiple values.
    ///
    /// # Allowed values
    ///
    /// - api
    /// - authentication
    /// - configuration
    /// - database
    /// - driver
    /// - email
    /// - file
    /// - host
    /// - iam
    /// - intrusion_detection
    /// - library
    /// - malware
    /// - network
    /// - package
    /// - process
    /// - registry
    /// - session
    /// - threat
    /// - vulnerability
    /// - web
    #[serde(skip_serializing_if = "Option::is_none")]
    category: Option<Vec<String>>,
    /// Type represents a categorization "sub-bucket" that allows events to be differentiated withing a
    /// category. Notably it is an array to allow for events that may be categorized under multiple values.
    ///
    /// # Allowed values
    ///
    /// - access
    /// - admin
    /// - allowed
    /// - change
    /// - connection
    /// - creation
    /// - deletion
    /// - denied
    /// - end
    /// - error
    /// - group
    /// - indicator
    /// - info
    /// - installation
    /// - protocol
    /// - start
    /// - user
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename(serialize = "type", deserialize = "type")
    )]
    type_: Option<Vec<String>>,
    /// Outcome simply denotes whether the event represents a success or a failure from the perspective of
    /// the entity that produced the event. It is the lowest-level event categorization in ECS.
    ///
    /// # Allowed values
    ///
    /// - failure
    /// - success
    /// - unknown
    #[serde(skip_serializing_if = "Option::is_none")]
    outcome: Option<String>,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub(crate) struct EcsLogFieldSet {
    /// Original log level of the log event.
    level: String,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub(crate) struct EcsHttpFieldSet {
    /// Fields related to an HTTP request.
    #[serde(skip_serializing_if = "Option::is_none")]
    request: Option<EcsHttpRequestFieldSet>,
    /// Fields related to an HTTP response.
    #[serde(skip_serializing_if = "Option::is_none")]
    response: Option<EcsHttpResponseFieldSet>,
    /// HTTP version.
    ///
    /// # Example
    ///
    /// `1.1`
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub(crate) struct EcsHttpRequestFieldSet {
    /// HTTP request method.
    ///
    /// # Example
    ///
    /// `POST`
    method: String,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub(crate) struct EcsHttpResponseFieldSet {
    /// HTTP status code.
    ///
    /// # Example
    ///
    /// `404`
    status: i64,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub(crate) struct EcsUrlFieldSet {
    /// The original URL value, as processed.
    original: String,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub(crate) struct EcsUserAgentFieldSet {
    /// The original user agent value, as processed.
    original: String,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub(crate) struct EcsRiskFieldSet {
    /// The risk classification calculated by the system.
    calculated_level: String,
    /// The risk score calculated by the system.
    calculated_risk_score: f64,
    /// The risk score calculated by the system, after normalization.
    #[serde(skip_serializing_if = "Option::is_none")]
    calculated_risk_score_norm: Option<f64>,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub(crate) struct EcsFileFieldSet {
    /// The full path to the file.
    ///
    /// # Example
    ///
    /// `/home/alice/plugins/example.wasm`
    path: String,
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub(crate) struct EcsBulwarkFieldSet {
    /// The combined decision accept value.
    #[serde(skip_serializing_if = "Option::is_none")]
    accept: Option<f64>,
    /// The combined decision restrict value.
    #[serde(skip_serializing_if = "Option::is_none")]
    restrict: Option<f64>,
    /// The combined decision unknown value.
    #[serde(skip_serializing_if = "Option::is_none")]
    unknown: Option<f64>,
    /// The decision components that contributed to the outcome.
    #[serde(skip_serializing_if = "Option::is_none")]
    plugins: Option<serde_json::Map<String, serde_json::Value>>,
}

impl std::fmt::Debug for EcsBulwarkFieldSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EcsBulwarkFieldSet")
            .field("accept", &self.accept)
            .field("restrict", &self.restrict)
            .field("unknown", &self.unknown)
            .field("plugins", &self.plugins)
            .finish()
    }
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub(crate) struct EcsBulwarkDecisionFieldSet {
    /// The plugin decision accept value.
    accept: f64,
    /// The plugin decision restrict value.
    restrict: f64,
    /// The plugin decision unknown value.
    unknown: f64,
    /// The plugin decision risk score.
    score: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracing_quoted_strings() -> Result<(), Box<dyn std::error::Error>> {
        let input = "\"They say, \\\"The quick brown fox jumped over the lazy dog.\\\"\"";
        let content = quoted_string::to_content::<TraceQuoteSpec>(input)?;
        assert_eq!(
            content,
            "They say, \"The quick brown fox jumped over the lazy dog.\""
        );

        Ok(())
    }
}
