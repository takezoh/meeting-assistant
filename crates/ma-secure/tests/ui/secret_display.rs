use ma_secure::Secret;

fn main() {
    let secret = Secret::new(String::from("ZZ-TOKEN-ZZ"));
    println!("{}", secret);
}
