use bulwark_sdk::*;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
struct Complex {
    color: String,
    size: i64,
}

pub struct SmokeTestPlugin;

impl SmokeTestPlugin {
    pub fn check_labels(labels: &HashMap<String, String>) -> Result<(), Error> {
        if !labels.contains_key("smoke") {
            return Err(error!("smoke label not set"));
        }
        Ok(())
    }

    pub fn check_response(response: &http::Response) -> Result<(), Error> {
        if !response.status().is_success() {
            return Err(error!("got non-success response from interior service"));
        }
        let body = String::from_utf8_lossy(response.body());
        if body.to_string().as_str() != "hello world" {
            return Err(error!(
                "got wrong response body from interior service: {}",
                body
            ));
        }
        Ok(())
    }

    pub fn check_http() -> Result<(), Error> {
        let outreq = http::request::Builder::new()
            .method("GET")
            .uri("http://gateway.docker.internal:5678/")
            .body(Bytes::new())?;
        let response = match http::send(outreq) {
            Ok(response) => response,
            Err(_) => {
                let outreq = http::request::Builder::new()
                    .method("GET")
                    .uri("http://localhost:5678/")
                    .body(Bytes::new())?;
                match http::send(outreq) {
                    Ok(response) => response,
                    Err(_) => {
                        let outreq = http::request::Builder::new()
                            .method("GET")
                            .uri("http://127.0.0.1:5678/")
                            .body(Bytes::new())?;
                        http::send(outreq)?
                    }
                }
            }
        };
        if !response.status().is_success() {
            return Err(error!("failed to get response from service"));
        }
        Ok(())
    }

    pub fn check_client_ip(req: &http::Request) -> Result<(), Error> {
        if let Some(ip) = client_ip(req) {
            if ip.to_string().as_str() != "1.2.3.4" {
                return Err(error!("got wrong client ip from request"));
            }
        } else {
            return Err(error!("failed to get client ip from request"));
        }
        Ok(())
    }

    pub fn check_config() -> Result<(), Error> {
        let keys = config_keys();
        if keys.len() != 2 {
            return Err(error!("got wrong number of config keys"));
        }
        if let Some(smoke) = config_var("smoke") {
            if smoke != value!(true) {
                return Err(error!("got wrong value for config var 'smoke'"));
            }
        } else {
            return Err(error!("failed to get config var 'smoke'"));
        }
        let complex: Complex =
            from_value(config_var("complex").ok_or(error!("failed to get config var 'complex'"))?)?;
        if complex.size != 13 {
            return Err(error!("got wrong value for config var 'complex.size'"));
        }
        if complex.color.as_str() != "blue" {
            return Err(error!("got wrong value for config var 'complex.color'"));
        }
        Ok(())
    }

    pub fn check_verdict(verdict: &Verdict) -> Result<(), Error> {
        if verdict.outcome != Outcome::Accepted {
            return Err(error!("got incorrect verdict outcome"));
        }
        if verdict.count != 1 {
            return Err(error!("got incorrect decision count"));
        }
        if !verdict.tags.is_empty() {
            return Err(error!("got incorrect verdict tags"));
        }
        if !verdict.decision.is_unknown() {
            return Err(error!("got incorrect verdict decision"));
        }
        Ok(())
    }
}

#[bulwark_plugin]
impl HttpHandlers for SmokeTestPlugin {
    fn handle_init() -> Result<(), Error> {
        Ok(())
    }

    fn handle_request_enrichment(
        _request: http::Request,
        _labels: HashMap<String, String>,
    ) -> Result<HashMap<String, String>, Error> {
        SmokeTestPlugin::check_config()?;
        Ok(HashMap::from([("smoke".to_string(), "true".to_string())]))
    }

    fn handle_request_decision(
        request: http::Request,
        labels: HashMap<String, String>,
    ) -> Result<HandlerOutput, Error> {
        SmokeTestPlugin::check_labels(&labels)?;
        SmokeTestPlugin::check_http()?;
        // don't need to test redis functions because the redis test already covers everything
        SmokeTestPlugin::check_client_ip(&request)?;
        SmokeTestPlugin::check_config()?;
        Ok(HandlerOutput::default())
    }

    fn handle_response_decision(
        request: http::Request,
        response: http::Response,
        labels: HashMap<String, String>,
    ) -> Result<HandlerOutput, Error> {
        SmokeTestPlugin::check_labels(&labels)?;
        SmokeTestPlugin::check_client_ip(&request)?;
        SmokeTestPlugin::check_config()?;
        SmokeTestPlugin::check_response(&response)?;
        Ok(HandlerOutput::default())
    }

    fn handle_decision_feedback(
        request: http::Request,
        response: http::Response,
        labels: HashMap<String, String>,
        verdict: Verdict,
    ) -> Result<(), Error> {
        SmokeTestPlugin::check_labels(&labels)?;
        SmokeTestPlugin::check_client_ip(&request)?;
        SmokeTestPlugin::check_config()?;
        SmokeTestPlugin::check_response(&response)?;
        SmokeTestPlugin::check_verdict(&verdict)?;
        Ok(())
    }
}
