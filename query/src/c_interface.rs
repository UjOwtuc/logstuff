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

pub struct Parsers {
    pub(crate) query: query::ExpressionParser,
    pub(crate) identifier: query::IdentifierParser,
    pub(crate) scalar: query::ScalarParser,
    pub(crate) list: query::ListParser,
    pub(crate) term: query::TermParser,
}

impl Parsers {
    fn new() -> Self {
        Self {
            query: query::ExpressionParser::new(),
            identifier: query::IdentifierParser::new(),
            scalar: query::ScalarParser::new(),
            list: query::ListParser::new(),
            term: query::TermParser::new(),
        }
    }
}

#[no_mangle]
pub extern "C" fn init_parsers() -> *mut Parsers {
    Box::into_raw(Box::new(Parsers::new()))
}

/// # Safety
/// C interface only. Do not use this in rust code.
#[no_mangle]
pub unsafe extern "C" fn delete_parsers(parsers: *mut Parsers) {
    drop(Box::from_raw(parsers));
}

/// # Safety
/// C interface only. Do not use this in rust code.
#[no_mangle]
pub unsafe extern "C" fn test_parse_query(parsers: *mut Parsers, text: *const c_char) -> i32 {
    let s = CStr::from_ptr(text).to_string_lossy().into_owned();
    match (*parsers).query.parse(&s) {
        Ok(_) => -1,
        Err(err) => location_from_error(err),
    }
}

/// # Safety
/// C interface only. Do not use this in rust code.
#[no_mangle]
pub unsafe extern "C" fn test_parse_identifier(parsers: *mut Parsers, text: *const c_char) -> i32 {
    let s = CStr::from_ptr(text).to_string_lossy().into_owned();
    match (*parsers).identifier.parse(&s) {
        Ok(_) => -1,
        Err(err) => location_from_error(err),
    }
}

/// # Safety
/// C interface only. Do not use this in rust code.
#[no_mangle]
pub unsafe extern "C" fn test_parse_scalar(parsers: *mut Parsers, text: *const c_char) -> i32 {
    let s = CStr::from_ptr(text).to_string_lossy().into_owned();
    match (*parsers).scalar.parse(&s) {
        Ok(_) => -1,
        Err(err) => location_from_error(err),
    }
}

/// # Safety
/// C interface only. Do not use this in rust code.
#[no_mangle]
pub unsafe extern "C" fn test_parse_list(parsers: *mut Parsers, text: *const c_char) -> i32 {
    let s = CStr::from_ptr(text).to_string_lossy().into_owned();
    match (*parsers).list.parse(&s) {
        Ok(_) => -1,
        Err(err) => location_from_error(err),
    }
}

/// # Safety
/// C interface only. Do not use this in rust code.
#[no_mangle]
pub unsafe extern "C" fn test_parse_term(parsers: *mut Parsers, text: *const c_char) -> i32 {
    let s = CStr::from_ptr(text).to_string_lossy().into_owned();
    match (*parsers).term.parse(&s) {
        Ok(_) => -1,
        Err(err) => location_from_error(err),
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn init_and_delete() {
        let p = init_parsers();
        let text = CStr::from_bytes_with_nul(b"#error\0").unwrap();
        unsafe {
            assert_eq!(test_parse_query(p, text.as_ptr()), 0);
            assert_eq!(test_parse_identifier(p, text.as_ptr()), 0);
            assert_eq!(test_parse_scalar(p, text.as_ptr()), 0);
            assert_eq!(test_parse_list(p, text.as_ptr()), 0);
            assert_eq!(test_parse_term(p, text.as_ptr()), 0);
            delete_parsers(p);
        }
    }
}
