use bulwark_wasm_sdk::*;

fn main() {
    let _request = get_request();
    set_decision(Decision {
        accept: 0.0,
        restrict: 0.0,
        unknown: 1.0,
    });
    set_tags(&[]);
}
