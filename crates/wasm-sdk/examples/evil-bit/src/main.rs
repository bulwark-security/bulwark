use bulwark_wasm_sdk::*;

fn main() {}

#[no_mangle]
fn on_request_decision() {
    let request = get_request();
    let evil_header = request.headers().get("Evil");
    if let Some(value) = evil_header {
        if value == "true" {
            set_decision(Decision {
                accept: 0.0,
                restrict: 1.0,
                unknown: 0.0,
            });
            set_tags(&["evil"]);
            return;
        }
    }
    set_decision(Decision {
        accept: 0.0,
        restrict: 0.0,
        unknown: 1.0,
    });
}
