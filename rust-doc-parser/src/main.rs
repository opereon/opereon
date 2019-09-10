use std::env;
use std::fs;

fn main() {
    let args: Vec<String> = env::args().collect();
    println!("{:?}", args);

    let file_to_load_path = &args[1];
    let file_to_save_path = &args[2];

    println!("File to load: {}", file_to_load_path);
    println!("File to save: {}", file_to_save_path);

    let content = fs::read_to_string(file_to_load_path)
        .expect("Something went wrong reading the file");

    println!("Content: {}", content);
}
