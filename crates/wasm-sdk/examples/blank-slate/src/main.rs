use bulwark_wasm_sdk::*;

fn main() {
    // Initialization logic goes here.
}

// Uncomment to implement cross-plugin communication logic.
// #[no_mangle]
// fn on_request() {
// }

#[no_mangle]
fn on_request_decision() {
    let _request = get_request();
    set_decision(Decision {
        accept: 0.0,
        restrict: 0.0,
        unknown: 1.0,
    })
    .expect("decision should be valid");
    set_tags([]);
}

// Uncomment to process responses from the interior service.
// #[no_mangle]
// fn on_response_decision() {
//     let _request = get_request();
//     let _response = get_response();
// }

// Uncomment to implement feedback loops.
// #[no_mangle]
// fn on_decision_feedback() {
// }
