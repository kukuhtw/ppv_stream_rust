
// src/validators.rs
use regex::Regex;

pub fn valid_email(s: &str) -> bool {
    let re = Regex::new(r"^[^@\s]+@[^@\s]+\.[^@\s]+$").unwrap();
    re.is_match(s)
}

pub fn valid_password(s: &str) -> bool {
    s.len() >= 8
}
