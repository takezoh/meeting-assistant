use ma_secure::{Content, LogField};

fn main() {
    let transcript = Content::new("ZZ-SECRET-CONTENT-ZZ");
    let _ = LogField::new("transcript", &transcript);
}
