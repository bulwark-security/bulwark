wit_bindgen_rust::import!("../../../../bulwark-host.wit");

fn main() {
    let request = bulwark_host::get_request();
    println!("Hello world. Method: {}", request.method);
}
