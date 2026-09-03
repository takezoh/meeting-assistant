//! `Secret<T>`: a value that renders as `***` under `Debug`, has no `Display` and no raw
//! `Serialize`, is zeroized on drop, and is reachable only through an explicit `expose()` at the
//! transmission site.

use std::fmt;
use zeroize::Zeroize;

pub struct Secret<T: Zeroize>(T);

impl<T: Zeroize> Secret<T> {
    pub fn new(value: T) -> Self {
        Secret(value)
    }

    /// The only way to read the value. Call it where the secret is transmitted, not before.
    pub fn expose(&self) -> &T {
        &self.0
    }
}

impl<T: Zeroize> Drop for Secret<T> {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl<T: Zeroize> fmt::Debug for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***")
    }
}

// Deliberately absent: `Display`, `Serialize`, `Clone`. A future `tracing` field or a
// serializer cannot regress the property because the code does not compile.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_renders_stars_and_expose_is_explicit() {
        let s = Secret::new(String::from("ZZ-TOKEN-ZZ"));
        assert_eq!(format!("{s:?}"), "***");
        assert_eq!(s.expose(), "ZZ-TOKEN-ZZ");
        let wrapped = (1, s);
        assert_eq!(
            format!("{wrapped:?}"),
            "(1, ***)",
            "nested debug output never reveals the value"
        );
    }
}
