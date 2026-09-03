use ma_secure::Secret;

fn main() {
    let secret = Secret::new(String::from("ZZ-TOKEN-ZZ"));
    let _ = serde_json::to_string(&secret).unwrap();
}
