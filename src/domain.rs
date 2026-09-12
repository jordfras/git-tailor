// Copyright 2026 Thomas Johannesson
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

pub mod commit;
pub mod diff;

use std::path::{Path, PathBuf};

/// A git index entry's path is raw bytes and need not be UTF-8; non-Unix
/// paths are UTF-8 in the index by construction, so the lossy round trip
/// there is exact in practice.
#[cfg(unix)]
pub(crate) fn path_to_bytes(path: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    path.as_os_str().as_bytes().to_vec()
}
#[cfg(not(unix))]
pub(crate) fn path_to_bytes(path: &Path) -> Vec<u8> {
    path.to_string_lossy().into_owned().into_bytes()
}

#[cfg(unix)]
pub(crate) fn bytes_to_path(bytes: &[u8]) -> PathBuf {
    use std::os::unix::ffi::OsStrExt;
    PathBuf::from(std::ffi::OsStr::from_bytes(bytes))
}
#[cfg(not(unix))]
pub(crate) fn bytes_to_path(bytes: &[u8]) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(bytes).into_owned())
}

/// Combine a target's message with a source's, the default text a squash or
/// fixup starts from: the target's message, a blank line, then the source's —
/// or just the target's when there is no source message to fold in (a fixup,
/// or a source with none of its own, such as a working-tree row).
pub fn combine_messages(target: &[u8], source: Option<&[u8]>) -> Vec<u8> {
    match source {
        Some(source) => {
            let mut combined = Vec::with_capacity(target.len() + source.len() + 2);
            combined.extend_from_slice(target);
            combined.extend_from_slice(b"\n\n");
            combined.extend_from_slice(source);
            combined
        }
        None => target.to_vec(),
    }
}

/// Serde for a commit message held as bytes.
///
/// git stores a message as bytes and most of them are UTF-8, so this writes a
/// plain JSON string whenever it can and falls back to an array of bytes when
/// it cannot. Reading accepts either, which is what lets a journal written
/// before messages became bytes load without a version bump or a migration.
pub(crate) mod message_bytes {
    use serde::de::{Error, SeqAccess, Visitor};
    use serde::{Deserializer, Serializer};
    use std::fmt;

    pub(crate) fn serialize<S: Serializer>(bytes: &[u8], ser: S) -> Result<S::Ok, S::Error> {
        match std::str::from_utf8(bytes) {
            Ok(text) => ser.serialize_str(text),
            Err(_) => ser.serialize_bytes(bytes),
        }
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<Vec<u8>, D::Error> {
        struct Either;

        impl<'de> Visitor<'de> for Either {
            type Value = Vec<u8>;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a commit message as a string or as bytes")
            }

            fn visit_str<E: Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(v.as_bytes().to_vec())
            }

            fn visit_bytes<E: Error>(self, v: &[u8]) -> Result<Self::Value, E> {
                Ok(v.to_vec())
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut out = Vec::new();
                while let Some(byte) = seq.next_element::<u8>()? {
                    out.push(byte);
                }
                Ok(out)
            }
        }

        de.deserialize_any(Either)
    }
}

/// Serde for a list of file paths, which git does not guarantee are UTF-8.
///
/// Each path uses the same string-or-bytes trick as [`message_bytes`],
/// applied independently per path so one non-UTF-8 entry does not force
/// every path in the list onto the byte-array shape.
pub(crate) mod path_list {
    use super::message_bytes;
    use super::{bytes_to_path, path_to_bytes};
    use serde::Deserialize;
    use serde::de::{SeqAccess, Visitor};
    use serde::ser::SerializeSeq;
    use serde::{Deserializer, Serializer};
    use std::fmt;
    use std::path::{Path, PathBuf};

    struct Elem<'a>(&'a Path);

    impl serde::Serialize for Elem<'_> {
        fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
            message_bytes::serialize(&path_to_bytes(self.0), ser)
        }
    }

    struct OnePath(PathBuf);

    impl<'de> Deserialize<'de> for OnePath {
        fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
            message_bytes::deserialize(de).map(|b| OnePath(bytes_to_path(&b)))
        }
    }

    pub(crate) fn serialize<S: Serializer>(paths: &[PathBuf], ser: S) -> Result<S::Ok, S::Error> {
        let mut seq = ser.serialize_seq(Some(paths.len()))?;
        for path in paths {
            seq.serialize_element(&Elem(path))?;
        }
        seq.end()
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<Vec<PathBuf>, D::Error> {
        struct Paths;

        impl<'de> Visitor<'de> for Paths {
            type Value = Vec<PathBuf>;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a list of file paths")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut out = Vec::new();
                while let Some(OnePath(p)) = seq.next_element()? {
                    out.push(p);
                }
                Ok(out)
            }
        }

        de.deserialize_seq(Paths)
    }
}

#[cfg(test)]
mod path_list_tests {
    use super::path_list;
    use std::path::PathBuf;

    #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
    struct Wrapper {
        #[serde(with = "path_list")]
        paths: Vec<PathBuf>,
    }

    #[cfg(unix)]
    fn non_utf8_path() -> PathBuf {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        // 0xFF is not a valid UTF-8 lead or continuation byte anywhere.
        let bytes = [b'b', b'a', b'd', 0xFF, b'.', b't', b'x', b't'];
        PathBuf::from(OsStr::from_bytes(&bytes))
    }

    #[test]
    fn utf8_paths_round_trip_as_plain_strings() {
        let wrapper = Wrapper {
            paths: vec![PathBuf::from("src/main.rs"), PathBuf::from("a/b.txt")],
        };
        let json = serde_json::to_string(&wrapper).unwrap();
        assert_eq!(json, r#"{"paths":["src/main.rs","a/b.txt"]}"#);
        let back: Wrapper = serde_json::from_str(&json).unwrap();
        assert_eq!(back, wrapper);
    }

    #[test]
    #[cfg(unix)]
    fn non_utf8_path_round_trips_through_a_byte_array_without_disturbing_its_neighbors() {
        let wrapper = Wrapper {
            paths: vec![PathBuf::from("ok.txt"), non_utf8_path()],
        };
        let json = serde_json::to_string(&wrapper).unwrap();
        // The UTF-8 neighbor keeps its plain-string shape; only the bad one
        // falls back to a byte array.
        assert!(json.starts_with(r#"{"paths":["ok.txt",["#), "got {json}");
        let back: Wrapper = serde_json::from_str(&json).unwrap();
        assert_eq!(back, wrapper);
    }

    #[test]
    #[cfg(unix)]
    fn a_journal_written_before_paths_could_be_non_utf8_still_loads() {
        // The old wire shape (plain strings) must still parse under the new
        // per-element scheme — that is the whole point of not bumping the
        // journal version for this.
        let json = r#"{"paths":["src/main.rs"]}"#;
        let wrapper: Wrapper = serde_json::from_str(json).unwrap();
        assert_eq!(wrapper.paths, vec![PathBuf::from("src/main.rs")]);
    }
}
