use std::iter;

/// `Zero width space` character.
pub const ZERO: char = '\u{200b}';
/// `Zero width non joiner` character.
pub const ONE: char = '\u{200c}';
/// `Zero width joiner` character.
pub const SEPARATOR: char = '\u{200d}';

const BINARY: &[char] = &[ZERO, ONE];
const DECIMAL: &[char] = &['0', '1', '2', '3', '4', '5', '6', '7', '8', '9'];

pub trait Tag {
    fn new(num: usize) -> Self;
    fn value(&self) -> usize;
    fn as_str(&self) -> &str;
    /// Return the first tag (if any).
    fn find(string: &str) -> Option<Self>
    where
        Self: Sized;
    fn separator() -> Option<&'static str> {
        None
    }
}

/// Binary encoded zero-width spaces, joiners, and non-joiners.
pub struct Binary(String);

impl Tag for Binary {
    fn new(num: usize) -> Self {
        let binary = format!("{:b}", num);
        let binary = binary.chars().map(|c| match c {
            '0' => ZERO,
            '1' => ONE,
            _ => unreachable!(),
        });
        let binary = iter::once(SEPARATOR)
            .chain(binary)
            .chain(iter::once(SEPARATOR))
            .collect::<String>();

        Self(binary)
    }

    fn value(&self) -> usize {
        let binary = self
            .0
            .trim_matches(SEPARATOR)
            .chars()
            .map(|c| match c {
                ZERO => '0',
                ONE => '1',
                _ => unreachable!(),
            })
            .collect::<String>();

        usize::from_str_radix(binary.as_str(), 2).expect("unreachable")
    }

    fn find(string: &str) -> Option<Self> {
        if string.is_empty() {
            None
        } else if let Some(first_separator) = string.find(SEPARATOR) {
            let string = &string[first_separator..];

            let tag = find_tag(string, BINARY);

            tag.map(|tag| Self(String::from(tag)))
        } else {
            None
        }
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

/// Decimal encoded numeric tag.
pub struct Decimal(String);

impl Tag for Decimal {
    fn new(num: usize) -> Self {
        let decimal = format!("{SEPARATOR}{num}{SEPARATOR}");

        Self(decimal)
    }

    fn value(&self) -> usize {
        let decimal = self.0.as_str().trim_matches(SEPARATOR);

        decimal.parse::<usize>().expect("unreachable")
    }

    fn find(string: &str) -> Option<Self> {
        if string.is_empty() {
            None
        } else if let Some(first_separator) = string.find(SEPARATOR) {
            let string = &string[first_separator..];

            let tag = find_tag(string, DECIMAL);

            tag.map(|tag| Self(String::from(tag)))
        } else {
            None
        }
    }

    fn as_str(&self) -> &str {
        &self.0
    }

    fn separator() -> Option<&'static str> {
        Some(": ")
    }
}

fn char_matches(c: char, matches: &[char]) -> bool {
    for &m in matches.iter() {
        if c == m {
            return true;
        }
    }

    false
}

fn find_tag<'a>(string: &'a str, valid_chars: &[char]) -> Option<&'a str> {
    enum State {
        Mismatch,
        Separator(usize),
        Match(usize),
        Close(usize),
        Found(usize, usize),
    }

    let mut state = State::Mismatch;
    for (i, c) in string.char_indices() {
        state = match (state, c) {
            (State::Match(start) | State::Separator(start), c) if char_matches(c, valid_chars) => {
                State::Match(start)
            }
            (State::Close(start), _) => {
                state = State::Found(start, i);
                break;
            }
            (State::Mismatch | State::Separator(_), SEPARATOR) => State::Separator(i),
            (State::Match(start), SEPARATOR) => State::Close(start),
            _ => State::Mismatch,
        };
    }

    match state {
        State::Found(start, end) => Some(&string[start..end]),
        State::Close(start) => Some(&string[start..]),
        _ => None,
    }
}
