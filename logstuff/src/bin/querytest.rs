use logstuff::query::parse_query;
use std::io::{self, BufRead};

fn main() {
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let line = line.unwrap();
        match parse_query(line.as_ref()) {
            Ok((expr, params)) => {
                println!("expression: {}", expr);
                println!("params:");
                params
                    .iter()
                    .enumerate()
                    .for_each(|pair| println!("\t${} = {:?}", pair.0 + 1, pair.1));
            }
            Err(err) => println!("Could not parse query: {:?}", err),
        }
    }
}
