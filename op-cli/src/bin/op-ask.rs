
fn main() {
    match std::env::var("OPEREON_PASSWD") {
        Ok(passwd) => println!("{}", passwd),
        Err(_) => println!(),
    }
}