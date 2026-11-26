use std::fmt::Write as _;
use std::fs;
use std::io::{Cursor, ErrorKind};
use std::path::{Component, Path, PathBuf};

use image::imageops::FilterType;
use image::ImageFormat;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::models::ClipoError;

/// Maximum byte length stored inline in SQLite before offloading to `blobs/`.
pub const INLINE_TEXT_LIMIT: usize = 64 * 1024;

/// Default maximum thumbnail height in pixels (2x Retina for 40px default row height).
pub const DEFAULT_THUMB_MAX_HEIGHT: u32 = 80;

/// Metadata produced when persisting an image and its PNG thumbnail.
#[derive(Debug, Clone)]
pub struct StoredImageBlob {
    pub blob_rel_path: String,
    pub thumb_rel_path: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub byte_size: i64,
}

#[derive(Debug, Serialize, Deserialize)]
struct RichPayloadBlob {
    rtf: Option<String>,
    html: Option<String>,
}

/// Computes the lowercase hexadecimal SHA-256 digest for `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(64);
    for byte in digest {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

/// Resolves a relative path inside `blobs_dir`, rejecting traversal or absolute escapes.
pub fn resolve_blob_path(blobs_dir: &Path, rel_path: &str) -> Result<PathBuf, ClipoError> {
    if rel_path.is_empty() {
        return Err(ClipoError::InvalidInput(
            "blob path cannot be empty".to_string(),
        ));
    }
    if rel_path.contains('\0')
        || rel_path.starts_with('/')
        || rel_path.starts_with('\\')
        || rel_path.contains('\\')
        || rel_path.contains(':')
    {
        return Err(ClipoError::InvalidInput(format!(
            "unsafe blob path rejected: {rel_path}"
        )));
    }

    let rel = Path::new(rel_path);
    let mut component_count = 0usize;
    for component in rel.components() {
        match component {
            Component::Normal(_) => component_count += 1,
            Component::Prefix(_)
            | Component::RootDir
            | Component::ParentDir
            | Component::CurDir => {
                return Err(ClipoError::InvalidInput(format!(
                    "unsafe blob path component in: {rel_path}"
                )));
            }
        }
    }

    if component_count == 0 {
        return Err(ClipoError::InvalidInput(format!(
            "invalid relative blob path: {rel_path}"
        )));
    }

    let joined = blobs_dir.join(rel);

    if blobs_dir.exists() && joined.exists() {
        let canon_base = fs::canonicalize(blobs_dir)?;
        let canon_target = fs::canonicalize(&joined)?;
        if !canon_target.starts_with(&canon_base) {
            return Err(ClipoError::InvalidInput(format!(
                "blob path escapes blobs directory: {rel_path}"
            )));
        }
    }

    Ok(joined)
}

/// Atomically writes `data` to `blobs_dir/rel_path` via a temporary sibling file.
pub fn write_blob_atomic(
    blobs_dir: &Path,
    rel_path: &str,
    data: &[u8],
) -> Result<PathBuf, ClipoError> {
    let target = resolve_blob_path(blobs_dir, rel_path)?;
    fs::create_dir_all(blobs_dir)?;

    if let Ok(meta) = fs::metadata(&target) {
        if meta.is_file() && meta.len() == data.len() as u64 {
            return Ok(target);
        }
    }

    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }

    let tmp_path = blobs_dir.join(format!(".tmp-{}", Uuid::new_v4()));
    if let Err(err) = fs::write(&tmp_path, data) {
        let _ = fs::remove_file(&tmp_path);
        return Err(ClipoError::Io(err));
    }
    if let Err(err) = fs::rename(&tmp_path, &target) {
        let _ = fs::remove_file(&tmp_path);
        return Err(ClipoError::Io(err));
    }

    Ok(target)
}

/// Reads raw bytes for a blob relative to `blobs_dir`, returning `NotFound` if missing.
pub fn read_blob(blobs_dir: &Path, rel_path: &str) -> Result<Vec<u8>, ClipoError> {
    let path = resolve_blob_path(blobs_dir, rel_path)?;
    match fs::read(&path) {
        Ok(bytes) => Ok(bytes),
        Err(err) if err.kind() == ErrorKind::NotFound => Err(ClipoError::NotFound(format!(
            "blob file missing on disk: {rel_path}"
        ))),
        Err(err) => Err(ClipoError::Io(err)),
    }
}

/// Reads a UTF-8 text blob relative to `blobs_dir`.
pub fn read_blob_utf8(blobs_dir: &Path, rel_path: &str) -> Result<String, ClipoError> {
    let bytes = read_blob(blobs_dir, rel_path)?;
    String::from_utf8(bytes)
        .map_err(|err| ClipoError::Storage(format!("invalid UTF-8 in blob {rel_path}: {err}")))
}

/// Removes a blob file if present on disk; ignores `NotFound`.
pub fn delete_blob_if_exists(blobs_dir: &Path, rel_path: &str) -> Result<(), ClipoError> {
    let path = resolve_blob_path(blobs_dir, rel_path)?;
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
        Err(err) => Err(ClipoError::Io(err)),
    }
}

/// Persists original image bytes and generates a PNG thumbnail scaled to `thumb_max_height`.
pub fn store_image_and_thumbnail(
    blobs_dir: &Path,
    bytes: &[u8],
    hint_width: Option<u32>,
    hint_height: Option<u32>,
    thumb_max_height: u32,
) -> Result<StoredImageBlob, ClipoError> {
    let hash = sha256_hex(bytes);
    let ext = match image::guess_format(bytes) {
        Ok(ImageFormat::Png) => "png",
        Ok(ImageFormat::Jpeg) => "jpg",
        Ok(ImageFormat::WebP) => "webp",
        _ => "bin",
    };

    let blob_rel_path = format!("{hash}.{ext}");
    write_blob_atomic(blobs_dir, &blob_rel_path, bytes)?;

    match image::load_from_memory(bytes) {
        Ok(img) => {
            let width = img.width();
            let height = img.height();
            let resized = if thumb_max_height > 0 && height > thumb_max_height {
                img.resize(u32::MAX, thumb_max_height, FilterType::Triangle)
            } else {
                img
            };

            let mut thumb_bytes = Vec::new();
            let mut cursor = Cursor::new(&mut thumb_bytes);
            resized
                .write_to(&mut cursor, ImageFormat::Png)
                .map_err(|err| {
                    ClipoError::Storage(format!("failed to encode PNG thumbnail: {err}"))
                })?;

            let thumb_rel_path = format!("{hash}.thumb.png");
            write_blob_atomic(blobs_dir, &thumb_rel_path, &thumb_bytes)?;

            Ok(StoredImageBlob {
                blob_rel_path,
                thumb_rel_path: Some(thumb_rel_path),
                width: Some(width),
                height: Some(height),
                byte_size: (bytes.len() + thumb_bytes.len()) as i64,
            })
        }
        Err(_) => Ok(StoredImageBlob {
            blob_rel_path,
            thumb_rel_path: None,
            width: hint_width,
            height: hint_height,
            byte_size: bytes.len() as i64,
        }),
    }
}

/// Writes oversized RTF/HTML payloads as a content-addressed JSON blob.
pub fn write_rich_blob(
    blobs_dir: &Path,
    rtf_payload: Option<&str>,
    html_payload: Option<&str>,
) -> Result<(String, usize), ClipoError> {
    let payload = RichPayloadBlob {
        rtf: rtf_payload.map(str::to_string),
        html: html_payload.map(str::to_string),
    };
    let json_bytes = serde_json::to_vec(&payload)?;
    let hash = sha256_hex(&json_bytes);
    let rel_path = format!("{hash}.rich.json");
    write_blob_atomic(blobs_dir, &rel_path, &json_bytes)?;
    Ok((rel_path, json_bytes.len()))
}

/// Reads an offloaded rich payload JSON blob back into `(rtf_payload, html_payload)`.
pub fn read_rich_blob(
    blobs_dir: &Path,
    rel_path: &str,
) -> Result<(Option<String>, Option<String>), ClipoError> {
    let bytes = read_blob(blobs_dir, rel_path)?;
    let parsed: RichPayloadBlob = serde_json::from_slice(&bytes)?;
    Ok((parsed.rtf, parsed.html))
}

/// Computes total disk bytes used inside `blobs_dir`, ignoring temporary write files.
pub fn total_blobs_bytes(blobs_dir: &Path) -> Result<u64, ClipoError> {
    if !blobs_dir.exists() {
        return Ok(0);
    }

    let mut total = 0u64;
    for entry in fs::read_dir(blobs_dir)? {
        let entry = entry?;
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if name.starts_with(".tmp-") {
            continue;
        }
        let meta = entry.metadata()?;
        if meta.is_file() {
            total = total.saturating_add(meta.len());
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_blob_path_rejects_traversal_and_absolute_paths() {
        let base = std::env::temp_dir().join("clipo-blobs-unit");
        assert!(resolve_blob_path(&base, "").is_err());
        assert!(resolve_blob_path(&base, "/etc/passwd").is_err());
        assert!(resolve_blob_path(&base, "../secret.txt").is_err());
        assert!(resolve_blob_path(&base, "sub/../../secret.txt").is_err());
        assert!(resolve_blob_path(&base, "C:\\Windows\\win.ini").is_err());
        assert!(resolve_blob_path(&base, "C:foo.png").is_err());
        assert!(resolve_blob_path(&base, "foo\\bar.png").is_err());
        assert!(resolve_blob_path(&base, "./foo.png").is_err());
        assert!(resolve_blob_path(&base, "abc\0.png").is_err());

        let valid = resolve_blob_path(&base, "deadbeef.png").expect("valid filename");
        assert_eq!(valid, base.join("deadbeef.png"));
    }
}
