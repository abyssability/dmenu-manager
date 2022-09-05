use std::{cell::RefCell, fmt::Write};

/// `Zero width space` character.
const ZERO: char = '\u{200b}';
/// `Zero width joiner` character.
const ONE: char = '\u{200d}';
/// `Zero width non joiner` character.
const SEP: char = '\u{200c}';

thread_local! {
    /// Persistant [`String`] buffer to minimize allocations.
    static BUF: RefCell<String> =
        String::with_capacity(usize::BITS.try_into().expect("unreachable")).into();
}

macro_rules! with_buf {
    ($buf:ident; $($token:tt)+) => {
        BUF.with(|buf| {
            let mut $buf = buf.borrow_mut();
            $buf.clear();
            $($token)+
        })
    }
}

/// Convert a number to a string tag, and convert that tag back to its numeric value.
pub trait Tag {
    /// Convert a number to a tag that is pushed onto the provided [`String`].
    fn push_tag(num: usize, out: &mut String);
    /// Convert the provided tag to its value as a [`usize`].
    fn convert_tag(tag: &str) -> Option<usize>;

    /// Find the first tag, returning it and any part of the string after the tag.
    fn pop_tag(string: &str) -> Option<usize> {
        string.find(SEP).and_then(|first_sep| {
            let start = first_sep + SEP.len_utf8();
            let string = &string[start..];
            string.find(SEP).and_then(|end| {
                let tag = &string[..end];
                Self::convert_tag(tag)
            })
        })
    }
}

/// Binary encoded zero-width spaces and joiners.
pub struct Binary;

impl Tag for Binary {
    fn push_tag(num: usize, out: &mut String) {
        with_buf! {buf;
            write!(buf, "{SEP}{num:b}{SEP}").unwrap();
            let binary = buf.chars().map(|c| match c {
                '0' => ZERO,
                '1' => ONE,
                SEP => SEP,
                _ => unreachable!(),
            });
            out.extend(binary);
        }
    }

    fn convert_tag(tag: &str) -> Option<usize> {
        let tag = tag.trim_matches(SEP);

        with_buf! {buf;
            let mut valid = true;
            let binary = tag.chars().map_while(|c| match c {
                ZERO => Some('0'),
                ONE => Some('1'),
                _ => {
                    valid = false;
                    None
                }
            });
            buf.extend(binary);

            if valid {
                usize::from_str_radix(&buf, 2).ok()
            } else {
                None
            }
        }
    }
}

/// Decimal encoded ascii.
pub struct Decimal;

impl Tag for Decimal {
    fn push_tag(num: usize, out: &mut String) {
        write!(out, "{SEP}{num}{SEP}").unwrap();
    }

    fn convert_tag(tag: &str) -> Option<usize> {
        let tag = tag.trim_matches(SEP);
        tag.parse().ok()
    }
}
