use super::*;


use std::borrow::Cow;
use std::ops::{Deref, DerefMut};
use std::str::FromStr;

use serde::{ser, de};
use crypto::sha1::Sha1;


#[derive(Debug, Clone, Copy)]
pub enum Sha1HashParseError {
    InvalidLength,
    InvalidValue,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Sha1Hash([u8; 20]);

impl Sha1Hash {
    pub fn nil() -> Self {
        Sha1Hash([0; 20])
    }

    pub fn is_nil(&self) -> bool {
        for &b in self.0.iter() {
            if b != 0u8 {
                return false;
            }
        }
        true
    }

    pub fn result(sha1: &mut Sha1) -> Sha1Hash {
        use crypto::digest::Digest;

        let mut h = Sha1Hash::nil();
        sha1.result(&mut h.0);
        h
    }

    pub fn as_oid(&self) -> git2::Oid {
        git2::Oid::from_bytes(&self.0).unwrap()
    }
}

impl Default for Sha1Hash {
    fn default() -> Self {
        Self::nil()
    }
}

impl Deref for Sha1Hash {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.0
    }
}

impl DerefMut for Sha1Hash {
    fn deref_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }
}

impl std::fmt::Display for Sha1Hash {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        for &b in self.0.iter() {
            write!(f, "{:02x}", b)?;
        }
        Ok(())
    }
}

impl FromStr for Sha1Hash {
    type Err = Sha1HashParseError;

    fn from_str(s: &'_ str) -> Result<Self, Self::Err> {
        if s.len() == 40 {
            let mut h = [0u8; 20];
            for (i, b) in s.as_bytes().chunks(2).enumerate() {
                let b1 = match (b[0] as char).to_digit(16) {
                    Some(b) => b as u8,
                    None => return Err(Sha1HashParseError::InvalidValue),
                };
                let b2 = match (b[1] as char).to_digit(16) {
                    Some(b) => b as u8,
                    None => return Err(Sha1HashParseError::InvalidValue),
                };
                h[i] = b1 * 16 + b2;
            }
            Ok(Sha1Hash(h))
        } else {
            Err(Sha1HashParseError::InvalidLength)
        }
    }
}

impl From<git2::Oid> for Sha1Hash {
    fn from(oid: git2::Oid) -> Self {
        let mut hash = Sha1Hash::nil();
        (*hash).copy_from_slice(oid.as_bytes());
        hash
    }
}

impl ser::Serialize for Sha1Hash {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where S: ser::Serializer {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> de::Deserialize<'de> for Sha1Hash {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: de::Deserializer<'de> {
        let s: String = de::Deserialize::deserialize(deserializer)?;
        match Self::from_str(&s) {
            Ok(h) => Ok(h),
            Err(err) => match err {
                Sha1HashParseError::InvalidLength => Err(serde::de::Error::invalid_length(s.len(), &"40")),
                Sha1HashParseError::InvalidValue => Err(serde::de::Error::invalid_value(serde::de::Unexpected::Str(&s), &"40 hexadecimal digits")),
            }
        }
    }
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    uid: usize,
    username: String,
}

impl User {
    pub fn new(uid: usize, username: Cow<str>) -> Self {
        User {
            uid,
            username: username.into(),
        }
    }

    pub fn current() -> Self {
        let uid = users::get_current_uid() as usize;
        let username = users::get_current_username().map_or(String::new(), |u| u.into_string().unwrap());
        User::new(uid, username.into())
    }

    pub fn uid(&self) -> usize {
        self.uid
    }

    pub fn username(&self) -> &str {
        &self.username
    }
}

impl Default for User {
    fn default() -> Self {
        User {
            uid: 0,
            username: String::new(),
        }
    }
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metadata {
    /// Model identifier as git Oid
    id: Sha1Hash,
    /// Path to model git repository
    path: PathBuf,
    user: User,
    timestamp: DateTime<Utc>,
    #[serde(skip)]
    stored: bool,
}

impl Metadata {
    pub fn new(id: Sha1Hash, path: PathBuf, user: User, timestamp: DateTime<Utc>) -> Metadata {
        Metadata {
            id,
            path,
            user,
            timestamp,
            stored: false,
        }
    }

    pub fn id(&self) -> Sha1Hash {
        self.id
    }

    pub (super) fn set_id(&mut self, id: Sha1Hash) {
        self.id = id;
    }

    pub fn user(&self) -> &User {
        &self.user
    }

    pub fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub (super) fn set_path(&mut self, path: PathBuf) {
        self.path = path;
    }

    pub fn is_stored(&self) -> bool {
        self.stored
    }

    pub fn set_stored(&mut self, stored: bool) {
        self.stored = stored;
    }
}

impl Default for Metadata {
    fn default() -> Self {
        Metadata {
            id: Sha1Hash::nil(),
            user: User::default(),
            timestamp: Utc.timestamp(0, 0),
            path: PathBuf::new(),
            stored: false,
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    fn example_meta() -> Metadata {
        Metadata::new(
            Sha1Hash::nil(),
            PathBuf::from("/home/example"),
            User::new(1000, "johnny".into()),
            Utc.timestamp(0, 0),
        )
    }

    #[test]
    fn serialize_json() {
        let m = example_meta();
        let res = serde_json::to_string(&m);
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), r#"{"id":"0000000000000000000000000000000000000000","path":"/home/example","user":{"uid":1000,"username":"johnny"},"timestamp":"1970-01-01T00:00:00Z"}"#)
    }

    #[test]
    fn serialize_yaml() {
        let m = example_meta();
        let res = serde_yaml::to_string(&m);
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), indoc!(r#"---
            id: "0000000000000000000000000000000000000000"
            path: /home/example
            user:
              uid: 1000
              username: johnny
            timestamp: "1970-01-01T00:00:00Z""#));
    }
}
