//! Reading parts out of ZIP-based document formats.

use deft_core::error::{ErrorCode, Result, err};
use std::io::{Cursor, Read};

/// Largest single part deft will unpack, against zip bombs.
const MAX_PART: u64 = 1 << 30;

pub struct Package {
    zip: std::cell::RefCell<zip::ZipArchive<Cursor<Vec<u8>>>>,
}

impl Package {
    pub fn open(bytes: &[u8]) -> Result<Package> {
        let zip = zip::ZipArchive::new(Cursor::new(bytes.to_vec()))
            .map_err(|e| err(ErrorCode::InputCorrupt, format!("not a valid package: {e}")))?;
        Ok(Package {
            zip: std::cell::RefCell::new(zip),
        })
    }

    pub fn names(&self) -> Vec<String> {
        self.zip.borrow().file_names().map(String::from).collect()
    }

    pub fn read(&self, name: &str) -> Option<Vec<u8>> {
        let mut zip = self.zip.borrow_mut();
        let file = zip.by_name(name).ok()?;
        let mut out = Vec::new();
        file.take(MAX_PART).read_to_end(&mut out).ok()?;
        Some(out)
    }

    pub fn text(&self, name: &str) -> Option<String> {
        let bytes = self.read(name)?;
        let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
        Some(String::from_utf8_lossy(bytes).to_string())
    }
}

/// Resolve `target` against the folder of `base`, handling `../`.
pub fn join(base: &str, target: &str) -> String {
    if let Some(abs) = target.strip_prefix('/') {
        return abs.to_string();
    }
    let mut parts: Vec<&str> = base.split('/').collect();
    parts.pop();
    for t in target.split('/') {
        match t {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            other => parts.push(other),
        }
    }
    parts.join("/")
}
