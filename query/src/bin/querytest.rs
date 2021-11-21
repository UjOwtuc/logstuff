use std::io::{self, BufRead};

use logstuff_query::ExpressionParser;

fn main() {
    let stdin = io::stdin();
    let parser = ExpressionParser::default();
    for line in stdin.lock().lines() {
        let line = line.unwrap();

        match parser.to_sql(line.as_ref()) {
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
