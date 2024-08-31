#![allow(missing_docs)]

use serde::{
    de::{Error, MapAccess},
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::{
    borrow::{Borrow, BorrowMut},
    fmt::{self, Formatter},
    marker::PhantomData,
};

pub(crate) const NAME: &str = "$__serde_private_Spanned";
pub(crate) const INDEX: &str = "$__serde_private_index";
pub(crate) const ROW: &str = "$__serde_private_row";
pub(crate) const COLUMN: &str = "$__serde_private_column";
pub(crate) const VALUE: &str = "$__serde_private_value";
pub(crate) const LEN: &str = "$__serde_private_len";

pub(crate) const FIELDS: &[&str] = &[INDEX, ROW, COLUMN, VALUE, LEN];

/// An wrapper which records the location of an item as byte indices into the
/// source text.
///
/// # Examples
///
/// Primitive values can be wrapped with [`Spanned<T>`] to get their location,
/// as you may expect.
///
/// ```rust
/// # use serde_yaml::Spanned;
/// #[derive(Debug, serde_derive::Deserialize)]
/// struct Document {
///     name: Spanned<String>,
/// }
/// # fn main() -> Result<(), serde_yaml::Error> {
///
/// let yaml = "name: Document";
///
/// let doc: Document = serde_yaml::from_str(yaml)?;
///
/// assert_eq!(doc.name.value, "Document");
/// assert_eq!(doc.name.start, 6);
/// assert_eq!(doc.name.len, "Document".len());
/// # Ok(())
/// # }
/// ```
///
/// More complex items like maps and arrays can also be used.
///
/// ```rust
/// # use serde_yaml::Spanned;
/// #[derive(Debug, serde_derive::Deserialize)]
/// struct Document {
///     words: Spanned<Vec<String>>,
/// }
/// # fn main() -> Result<(), serde_yaml::Error> {
///
/// let yaml = "words: [Hello, World]";
///
/// let doc: Document = serde_yaml::from_str(yaml)?;
///
/// assert_eq!(doc.words.value, &["Hello", "World"]);
/// assert_eq!(doc.words.start, yaml.find("[").unwrap());
/// assert_eq!(doc.words.len, "[Hello, World]".len());
/// assert_eq!(doc.words.end(), yaml.find("]").unwrap());
/// # Ok(())
/// # }
/// ```
///
/// Note that a map item starts after first key.
///
/// ```rust
/// # use serde_yaml::Spanned;
/// #[derive(Debug, serde_derive::Deserialize)]
/// struct Document {
///     nested: Spanned<Nested>,
/// }
/// #[derive(Debug, serde_derive::Deserialize)]
/// struct Nested {
///    first: u32,
///    second: u32,
/// }
/// # fn main() -> Result<(), serde_yaml::Error> {
///
/// let yaml = "nested:\n  first: 1\n  second: 2";
/// let doc: Document = serde_yaml::from_str(yaml)?;
///
/// let spanned_text = &yaml[doc.nested.span()];
/// assert_eq!(spanned_text, ": 1\n  second: 2");
/// # Ok(())
/// # }
/// ```
///
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct Spanned<T> {
    pub value: T,
    pub index: usize,
    pub line: usize,
    pub column: usize,
    pub len: usize,
}

impl<T> Spanned<T> {
    pub const fn new(index: usize, line: usize, column: usize, len: usize, value: T) -> Self {
        Spanned {
            value,
            index,
            line,
            column,
            len,
        }
    }

    pub const fn index(&self) -> usize {
        self.index
    }

    pub const fn line(&self) -> usize {
        self.line
    }

    pub const fn column(&self) -> usize {
        self.column
    }

    pub const fn len(&self) -> usize {
        self.len
    }
}

impl<T, Q> AsRef<Q> for Spanned<T>
where
    T: AsRef<Q>,
{
    fn as_ref(&self) -> &Q {
        self.value.as_ref()
    }
}

impl<T, Q> AsMut<Q> for Spanned<T>
where
    T: AsMut<Q>,
{
    fn as_mut(&mut self) -> &mut Q {
        self.value.as_mut()
    }
}

impl<T> Borrow<T> for Spanned<T> {
    fn borrow(&self) -> &T {
        &self.value
    }
}

impl<T> BorrowMut<T> for Spanned<T> {
    fn borrow_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

impl<T: Serialize> Serialize for Spanned<T> {
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        self.value.serialize(ser)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Spanned<T> {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        de.deserialize_struct(NAME, FIELDS, Visitor(PhantomData))
    }
}

struct Visitor<T>(PhantomData<T>);

impl<'de, T> serde::de::Visitor<'de> for Visitor<T>
where
    T: Deserialize<'de>,
{
    type Value = Spanned<T>;

    fn expecting(&self, formatter: &mut Formatter) -> fmt::Result {
        write!(formatter, "A spanned {}", core::any::type_name::<T>())
    }

    fn visit_map<A>(self, mut visitor: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        if visitor.next_key()? != Some(INDEX) {
            return Err(Error::custom("spanned index key not found"));
        }

        let index: usize = visitor.next_value()?;

        if visitor.next_key()? != Some(ROW) {
            return Err(Error::custom("spanned row key not found"));
        }

        let row: usize = visitor.next_value()?;

        if visitor.next_key()? != Some(COLUMN) {
            return Err(Error::custom("spanned column key not found"));
        }

        let column: usize = visitor.next_value()?;

        if visitor.next_key()? != Some(VALUE) {
            return Err(Error::custom("spanned value key not found"));
        }

        let value: T = visitor.next_value()?;

        if visitor.next_key()? != Some(LEN) {
            return Err(Error::custom("spanned len key not found"));
        }

        let len: usize = visitor.next_value()?;

        Ok(Spanned::new(index, row, column, len, value))
    }
}
