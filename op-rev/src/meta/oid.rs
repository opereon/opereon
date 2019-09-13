use std::ops::{Deref, DerefMut};
use std::str::FromStr;
use serde::{ser, de};


#[derive(Debug, Clone, Copy)]
pub enum OidParseError {
    InvalidLength,
    InvalidValue,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Oid([u8; 20]);

impl Oid {
    pub fn nil() -> Self {
        Oid([0; 20])
    }

    pub fn is_nil(&self) -> bool {
        for &b in self.0.iter() {
            if b != 0u8 {
                return false;
            }
        }
        true
    }
}

impl Default for Oid {
    fn default() -> Self {
        Self::nil()
    }
}

impl Deref for Oid {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.0
    }
}

impl DerefMut for Oid {
    fn deref_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }
}

impl std::fmt::Display for Oid {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if let Some(mut width) = f.width() {
            for &b in self.0.iter() {
                if width >= 2 {
                    write!(f, "{:02x}", b)?;
                    width -= 2;
                } else if width == 1 {
                    write!(f, "{:01x}", (b >> 4))?;
                    break;
                } else {
                    break;
                }
            }
        } else {
            for &b in self.0.iter() {
                write!(f, "{:02x}", b)?;
            }
        }
        Ok(())
    }
}

impl FromStr for Oid {
    type Err = OidParseError;

    fn from_str(s: &'_ str) -> Result<Self, Self::Err> {
        if s.len() == 40 {
            let mut h = [0u8; 20];
            for (i, b) in s.as_bytes().chunks(2).enumerate() {
                let b1 = match (b[0] as char).to_digit(16) {
                    Some(b) => b as u8,
                    None => return Err(OidParseError::InvalidValue),
                };
                let b2 = match (b[1] as char).to_digit(16) {
                    Some(b) => b as u8,
                    None => return Err(OidParseError::InvalidValue),
                };
                h[i] = b1 * 16 + b2;
            }
            Ok(Oid(h))
        } else {
            Err(OidParseError::InvalidLength)
        }
    }
}

impl ser::Serialize for Oid {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: ser::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> de::Deserialize<'de> for Oid {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: de::Deserializer<'de>,
    {
        let s: String = de::Deserialize::deserialize(deserializer)?;
        match Self::from_str(&s) {
            Ok(h) => Ok(h),
            Err(err) => match err {
                OidParseError::InvalidLength => {
                    Err(serde::de::Error::invalid_length(s.len(), &"40"))
                }
                OidParseError::InvalidValue => Err(serde::de::Error::invalid_value(
                    serde::de::Unexpected::Str(&s),
                    &"40 hexadecimal digits",
                )),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_compatible_with_git() {
        let oid_str = "0123456789abcdef0123456789abcdef01234567";

        let git_oid1: git2::Oid = git2::Oid::from_str(oid_str).unwrap();
        let oid1: Oid = git_oid1.into();

        let oid2: Oid = Oid::from_str(oid_str).unwrap();
        let git_oid2: git2::Oid = oid2.into();

        assert_eq!(oid1, oid2);
        assert_eq!(git_oid1, git_oid2);
    }
}