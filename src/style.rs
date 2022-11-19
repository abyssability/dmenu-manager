use std::io;

use is_terminal::IsTerminal;
use termcolor::{ColorChoice, ColorSpec, StandardStream, WriteColor};

pub fn bold() -> ColorSpec {
    let mut style = ColorSpec::new();
    style.set_bold(true);
    style
}

pub fn stderr_color_choice() -> ColorChoice {
    if io::stderr().is_terminal() {
        ColorChoice::Auto
    } else {
        ColorChoice::Never
    }
}

pub fn stderr_color_enabled() -> bool {
    io::stderr().is_terminal() && StandardStream::stderr(ColorChoice::Auto).supports_color()
}

pub fn stdout_color_enabled() -> bool {
    io::stdout().is_terminal() && StandardStream::stdout(ColorChoice::Auto).supports_color()
}

#[macro_export]
macro_rules! style_stderr {
    ($style:expr, $($token:tt)+) => {
        if $crate::style::stderr_color_enabled() {
            let mut buf = termcolor::Ansi::new(Vec::new());
            $crate::write_style!(buf, $style, $($token)+);
            String::from_utf8(buf.into_inner()).unwrap()
        } else {
            format!($($token)+)
        }
    }
}

#[macro_export]
macro_rules! style_stdout {
    ($style:expr, $($token:tt)+) => {
        if $crate::style::stdout_color_enabled() {
            let mut buf = termcolor::Ansi::new(Vec::new());
            $crate::write_style!(buf, $style, $($token)+);
            String::from_utf8(buf.into_inner()).unwrap()
        } else {
            format!($($token)+)
        }
    }
}

#[macro_export]
macro_rules! write_style {
    ($stream:ident, $style:expr, $($token:tt)+) => {
        {
            use termcolor::WriteColor;
            use std::io::Write;

            $stream.set_color(&$style).unwrap();
            write!(&mut $stream, $($token)+).unwrap();
            $stream.reset().unwrap();
        }
    }
}

pub use style_stderr;
pub use style_stdout;
pub use write_style;
