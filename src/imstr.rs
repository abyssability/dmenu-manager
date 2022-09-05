use std::{
    borrow::{Borrow, Cow},
    cmp::Ordering,
    fmt::{self, Display},
    hash::{Hash, Hasher},
    ops::Deref,
    rc::Rc,
};

/// Immutable string that is cheap to Clone.
///
/// Use [`Self::from`] to create heap allocated strings from [`String`], [`&str`], or [`Rc<str>`].
/// To create a static, unallocated string, use [`Self::new`].
///
/// ```
/// use crate::imstr::ImStr;
///
/// let string = String::from("Hello, world!");
/// let unallocated = ImStr::new("Hello, world!");
/// let immutable = ImStr::from(string);
/// ```
#[derive(Debug, Clone)]
pub enum ImStr {
    Heap(Rc<str>),
    Static(&'static str),
}

impl ImStr {
    pub const fn new(string: &'static str) -> Self {
        Self::Static(string)
    }

    pub fn as_str(&self) -> &str {
        self
    }
}

macro_rules! imstr_impl_from {
    ($($type:ty),+) => {
        $(
            impl From<$type> for ImStr {
                fn from(other: $type) -> Self {
                    Self::from(Into::<Rc<str>>::into(other))
                }
            }

            impl From<&$type> for ImStr {
                fn from(other: &$type) -> Self {
                    Self::from(AsRef::<str>::as_ref(other))
                }
            }
        )+
    }
}

imstr_impl_from!(String, Box<str>, Cow<'_, str>);

impl From<&Rc<str>> for ImStr {
    fn from(other: &Rc<str>) -> Self {
        Self::Heap(other.clone())
    }
}

impl From<Rc<str>> for ImStr {
    fn from(other: Rc<str>) -> Self {
        Self::Heap(other)
    }
}

impl From<&str> for ImStr {
    fn from(other: &str) -> Self {
        Self::Heap(other.into())
    }
}

impl Default for ImStr {
    fn default() -> Self {
        Self::Static("")
    }
}

impl AsRef<str> for ImStr {
    fn as_ref(&self) -> &str {
        self
    }
}

impl Borrow<str> for ImStr {
    fn borrow(&self) -> &str {
        self
    }
}

impl Deref for ImStr {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Heap(string) => string,
            Self::Static(string) => string,
        }
    }
}

impl Display for ImStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self)
    }
}

impl PartialEq for ImStr {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for ImStr {}

impl PartialOrd for ImStr {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ImStr {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl Hash for ImStr {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}
