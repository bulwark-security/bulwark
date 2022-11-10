use bulwark_wasm_sdk::interface;

fn main() {
    let request = interface::get_request();
    println!("Hello world. Method: {}", request.method);
}
