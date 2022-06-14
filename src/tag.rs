use std::{cell::RefCell, fmt::Write, iter};

/// `Zero width space` character.
const ZERO: char = '\u{200b}';
/// `Zero width non joiner` character.
const ONE: char = '\u{200c}';
/// `Zero width joiner` character.
const SEP: char = '\u{200d}';
const SEP_LEN: usize = SEP.len_utf8();

const BINARY: &[char] = &[ZERO, ONE];
const DECIMAL: &[char] = &['0', '1', '2', '3', '4', '5', '6', '7', '8', '9'];

thread_local! {
    static BUF: RefCell<String> =
        String::with_capacity(usize::BITS.try_into().expect("unreachable")).into();
}

macro_rules! with_buf {
    ($buf:ident, $($t:tt)*) => {
        BUF.with(|cell| {
            let mut $buf = cell.borrow_mut();
            $buf.clear();
            $($t)*
        })
    };
}

pub trait Tag {
    fn push_tag(num: usize, out: &mut String);
    fn convert_tag(tag: &str) -> Option<usize>;

    fn pop_tag(string: &str) -> Option<(usize, &str)> {
        string.find(SEP).and_then(|first_sep| {
            let string = &string[first_sep + SEP_LEN..];
            string.find(SEP).and_then(|second_sep| {
                let tag = &string[..second_sep];
                let tag = Self::convert_tag(tag);
                let string = &string[second_sep + SEP_LEN..];
                tag.map(|tag| (tag, string))
            })
        })
    }

    fn separator() -> Option<&'static str> {
        None
    }
}

/// Binary encoded zero-width spaces, joiners, and non-joiners.
pub struct Binary(String);

impl Tag for Binary {
    fn push_tag(num: usize, out: &mut String) {
        with_buf! {buf,
            write!(buf, "{num:b}").expect("formatting error");
            let binary = buf.chars().map(|c| match c {
                '0' => ZERO,
                '1' => ONE,
                _ => unreachable!(),
            });
            let binary = iter::once(SEP).chain(binary).chain(iter::once(SEP));

            out.extend(binary)
        }
    }

    fn convert_tag(tag: &str) -> Option<usize> {
        tag.chars()
            .all(|c| BINARY.contains(&c))
            .then(|| {
                let binary = tag.chars().map(|c| match c {
                    ZERO => '0',
                    ONE => '1',
                    _ => unreachable!(),
                });

                with_buf! {buf,
                    buf.extend(binary);
                    usize::from_str_radix(buf.as_str(), 2).ok()
                }
            })
            .flatten()
    }
}

/// Decimal encoded numeric tag.
pub struct Decimal(String);

impl Tag for Decimal {
    fn push_tag(num: usize, out: &mut String) {
        write!(*out, "{SEP}{num}{SEP}").expect("formatting error");
    }

    fn convert_tag(tag: &str) -> Option<usize> {
        tag.chars()
            .all(|c| DECIMAL.contains(&c))
            .then(|| tag.parse().ok())
            .flatten()
    }

    fn separator() -> Option<&'static str> {
        Some(": ")
    }
}
