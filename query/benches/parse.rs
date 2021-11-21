use criterion::{black_box, criterion_group, criterion_main, Criterion};
use logstuff_query::query;

pub fn parse_expression(c: &mut Criterion) {
    let p = query::ExpressionParser::new();
    c.bench_function("simple_expression", |b| {
        b.iter(|| p.parse(black_box(r#""bb" or not "bb""#)))
    });
}

pub fn parse_identifier(c: &mut Criterion) {
    let p = query::IdentifierParser::new();
    c.bench_function("simple_variable", |b| {
        b.iter(|| p.parse(black_box(r#"vars.DST"#)))
    });
    c.bench_function("long_variable", |b| {
        b.iter(|| p.parse(black_box(r#"vars.event.something.else.key_name"#)))
    });
}

pub fn parse_list(c: &mut Criterion) {
    let p = query::ListParser::new();
    c.bench_function("empty_list", |b| b.iter(|| p.parse(black_box(r#"()"#))));
    c.bench_function("short_int_list", |b| {
        b.iter(|| p.parse(black_box(r#"(1, 2, 3)"#)))
    });
    c.bench_function("short_mixed_list", |b| {
        b.iter(|| p.parse(black_box(r#"(1, 2.2, "three")"#)))
    });
    c.bench_function("long_mixed_list", |b| {
        b.iter(|| {
            p.parse(black_box(
                r#"(1, 2.2, "three", 4, 5.5, "six", 7, 8.8099001, "nine, I think")"#,
            ))
        })
    });
}

pub fn parse_scalar(c: &mut Criterion) {
    let p = query::ScalarParser::new();
    c.bench_function("zero", |b| b.iter(|| p.parse(black_box(r#"0"#))));
    c.bench_function("int", |b| b.iter(|| p.parse(black_box(r#"42"#))));
    c.bench_function("float", |b| {
        b.iter(|| p.parse(black_box(r#"3.14159265359"#)))
    });
    c.bench_function("quoted_string", |b| {
        b.iter(|| p.parse(black_box(r#""test string""#)))
    });
    c.bench_function("quoted_string_with_escapes", |b| {
        b.iter(|| p.parse(black_box(r#""some\ttest\rstring\"\n""#)))
    });
}

pub fn parse_term(c: &mut Criterion) {
    let p = query::TermParser::new();
    c.bench_function("match_scalar", |b| {
        b.iter(|| p.parse(black_box(r#"id = 42"#)))
    });
    c.bench_function("match_list", |b| {
        b.iter(|| p.parse(black_box(r#"id in (1.0, 42, "something")"#)))
    });
}

criterion_group!(
    benches,
    parse_expression,
    parse_identifier,
    parse_list,
    parse_scalar,
    parse_term
);
criterion_main!(benches);
