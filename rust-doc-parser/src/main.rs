use std::env;
use std::fs;
use structopt::StructOpt;
use std::path::PathBuf;
use std::io::{BufReader, BufRead};
use std::fmt::Write;

pub struct Parser<'a> {
    reader: BufReader<&'a [u8]>,
    buf: String,
    out: String,
    code: bool
}

impl Parser <'_>{
    pub fn new(reader: BufReader<&[u8]>) -> Parser {
        Parser {
            reader,
            buf: String::new(),
            out: String::new(),
            code: false
        }
    }
    pub fn next_line(&mut self) -> Option<String> {
        let r= self.reader.read_line(&mut self.buf).expect("Cannot read line");

        if r == 0 {
            return None
        } else {
            let line = self.buf.to_owned();
            self.buf.clear();
            Some(line)
        }
    }
    pub fn parse(&mut self) -> String {
        while let Some(line) = self.next_line() {
            if line.starts_with("```") {
                if self.code == false {
                    self.code = true;
//                    write!(self.out, "{}", "{{<code file=\"_.json\">}}\n").unwrap()
                } else {
                    self.code = false;
//                    write!(self.out, "{}", "{{</code>}}\n").unwrap()
                }
            } else if self.code == true {
                //TODO MC If line with "let model = r#"...", add line "{{<code file=\"_.json\">}}\n", add another lines with json, add line "{{</code>}}\n"
                //TODO MC If line with "let query..." do what need to be done
                //TODO MC If line with "let result..." do what need to be done
            } else {
                write!(self.out, "{}", line).unwrap()
            }
        }
        let mut out = String::new();
        std::mem::swap(&mut out, &mut self.out);
        out
    }
}


fn main() {
    let opt = Opt::from_args();
    println!("{:?}", opt);

    let file_to_load_path = opt.input;
    let file_to_save_path = opt.output;

    println!("File to load: {}", file_to_load_path.display());
    println!("File to save: {}", file_to_save_path.display());

    let content = fs::read_to_string(file_to_load_path)
        .expect("Something went wrong reading the file");

    let mut parser = Parser::new(BufReader::new(content.as_bytes()));

    let out = parser.parse();

//    std::fs::write("", out).unwrap()
    // save out to file
    println!("out: {}", out);
}

#[derive(Debug, StructOpt)]
#[structopt(name = "example", about = "An example of StructOpt usage.")]
struct Opt {
    /// Input file
    #[structopt(parse(from_os_str))]
    input: PathBuf,

    /// Output file, stdout if not present
    #[structopt(parse(from_os_str))]
    output: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foo() {

    }
}