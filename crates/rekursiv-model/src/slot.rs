//! Explicit host layout for optional hardware registers and resident metadata.
//!
//! This is the same valid-bit plus payload state as `Option<T>`. Its explicit
//! layout lets generated hardware emulation access the authoritative model
//! directly, without depending on Rust enum niches or maintaining a shadow pager.

/// An optional hardware value with an explicit validity bit.
///
/// Absent payloads are initialized but have no architectural significance.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Slot<T> {
    /// Payload; consume only when `present` is true.
    pub value: T,
    /// Whether the register or mapping is installed.
    pub present: bool,
}
impl<T: Default> Default for Slot<T> {
    fn default() -> Self {
        Self {
            value: T::default(),
            present: false,
        }
    }
}
impl<T: PartialEq> PartialEq for Slot<T> {
    fn eq(&self, other: &Self) -> bool {
        self.present == other.present && (!self.present || self.value == other.value)
    }
}
impl<T: Eq> Eq for Slot<T> {}
impl<T: Default> From<Option<T>> for Slot<T> {
    fn from(value: Option<T>) -> Self {
        match value {
            Some(value) => Self {
                value,
                present: true,
            },
            None => Self::default(),
        }
    }
}
impl<T> Slot<T> {
    /// Borrow an installed payload.
    pub fn as_ref(&self) -> Option<&T> {
        self.present.then_some(&self.value)
    }
    /// Borrow an installed payload mutably.
    pub fn as_mut(&mut self) -> Option<&mut T> {
        self.present.then_some(&mut self.value)
    }
    /// Whether a value is installed.
    pub fn is_some(&self) -> bool {
        self.present
    }
    /// Whether the slot is empty.
    pub fn is_none(&self) -> bool {
        !self.present
    }
}
impl<T: Copy> Slot<T> {
    /// Copy the logical optional value.
    pub fn get(self) -> Option<T> {
        self.present.then_some(self.value)
    }
    /// Read a required installed value.
    pub fn unwrap(self) -> T {
        self.get().unwrap()
    }
    /// Read an installed value or a default.
    pub fn unwrap_or(self, fallback: T) -> T {
        self.get().unwrap_or(fallback)
    }
    /// Filter an installed value.
    pub fn filter(self, f: impl FnOnce(&T) -> bool) -> Option<T> {
        self.get().filter(f)
    }
    /// Map an installed value.
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Option<U> {
        self.get().map(f)
    }
    /// Require an installed value, or return the supplied error.
    pub fn ok_or<E>(self, error: E) -> Result<T, E> {
        self.get().ok_or(error)
    }
    /// Test an installed value.
    pub fn is_some_and(self, f: impl FnOnce(T) -> bool) -> bool {
        self.get().is_some_and(f)
    }
}
impl<'a, T> IntoIterator for &'a Slot<T> {
    type Item = &'a T;
    type IntoIter = std::option::IntoIter<&'a T>;
    fn into_iter(self) -> Self::IntoIter {
        self.as_ref().into_iter()
    }
}
impl<'a, T> IntoIterator for &'a mut Slot<T> {
    type Item = &'a mut T;
    type IntoIter = std::option::IntoIter<&'a mut T>;
    fn into_iter(self) -> Self::IntoIter {
        self.as_mut().into_iter()
    }
}
