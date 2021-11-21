use criterion::{black_box, criterion_group, criterion_main, Criterion};
use logstuff_query::ExpressionParser;

pub fn parse_expression(c: &mut Criterion) {
    let p = ExpressionParser::default();
    c.bench_function("parse_expression", |b| {
        b.iter(|| p.to_sql(black_box(r#""bb" or not "bb""#)))
    });
}

criterion_group!(benches, parse_expression);
criterion_main!(benches);
