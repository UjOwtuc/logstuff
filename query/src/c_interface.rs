use lalrpop_util::ParseError;
use std::ffi::CStr;
use std::os::raw::c_char;

use crate::query;

fn location_from_error<T, E>(err: ParseError<usize, T, E>) -> i32 {
    use lalrpop_util::ParseError::*;
    let location = match err {
        InvalidToken { location } => location,
        UnrecognizedEOF {
            location,
            expected: _,
        } => location,
        UnrecognizedToken { token, expected: _ } => token.0,
        ExtraToken { token } => token.0,
        _ => 0,
    };
    location.try_into().unwrap_or(0)
}

/// # Safety
/// C interface only. Do not use this in rust code.
#[no_mangle]
pub unsafe extern "C" fn test_parse_query(text: *const c_char) -> i32 {
    let s = CStr::from_ptr(text).to_string_lossy().into_owned();
    match query::ExpressionParser::new().parse(&s) {
        Ok(_) => -1,
        Err(err) => location_from_error(err),
    }
}

/// # Safety
/// C interface only. Do not use this in rust code.
#[no_mangle]
pub unsafe extern "C" fn test_parse_identifier(text: *const c_char) -> i32 {
    let s = CStr::from_ptr(text).to_string_lossy().into_owned();
    match query::IdentifierParser::new().parse(&s) {
        Ok(_) => -1,
        Err(err) => location_from_error(err),
    }
}

/// # Safety
/// C interface only. Do not use this in rust code.
#[no_mangle]
pub unsafe extern "C" fn test_parse_scalar(text: *const c_char) -> i32 {
    let s = CStr::from_ptr(text).to_string_lossy().into_owned();
    match query::ScalarParser::new().parse(&s) {
        Ok(_) => -1,
        Err(err) => location_from_error(err),
    }
}

/// # Safety
/// C interface only. Do not use this in rust code.
#[no_mangle]
pub unsafe extern "C" fn test_parse_list(text: *const c_char) -> i32 {
    let s = CStr::from_ptr(text).to_string_lossy().into_owned();
    match query::ListParser::new().parse(&s) {
        Ok(_) => -1,
        Err(err) => location_from_error(err),
    }
}

/// # Safety
/// C interface only. Do not use this in rust code.
#[no_mangle]
pub unsafe extern "C" fn test_parse_term(text: *const c_char) -> i32 {
    let s = CStr::from_ptr(text).to_string_lossy().into_owned();
    match query::TermParser::new().parse(&s) {
        Ok(_) => -1,
        Err(err) => location_from_error(err),
    }
}
