use logstuff_query::{parse_query, try_parse, ParseRule};
use std::io::{self, BufRead};

fn main() {
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let line = line.unwrap();

        if let Err(pos) = try_parse(ParseRule::Query, line.as_ref()) {
            println!("Could not parse query, first error was at char {}", pos);
        }
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
