//! Archive capabilities: create, extract and list zip, tar and compressed
//! tarballs, and read 7z.

use deft_core::error::{ErrorCode, Result, err};
use deft_core::format::Format;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Component, Path, PathBuf};

fn corrupt(p: &Path, e: impl std::fmt::Display) -> deft_core::error::DeftError {
    err(ErrorCode::InputCorrupt, format!("{}: {e}", p.display()))
}

/// All files under the given paths, as (absolute path, name inside the archive).
fn collect(paths: &[PathBuf]) -> Result<Vec<(PathBuf, String)>> {
    fn walk(dir: &Path, prefix: &str, out: &mut Vec<(PathBuf, String)>) -> Result<()> {
        let mut entries: Vec<_> = std::fs::read_dir(dir)?.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let name = format!("{prefix}/{}", e.file_name().to_string_lossy());
            let path = e.path();
            if path.is_dir() {
                walk(&path, &name, out)?;
            } else if path.is_file() {
                out.push((path, name));
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    for p in paths {
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "root".into());
        if p.is_dir() {
            walk(p, &name, &mut out)?;
        } else {
            out.push((p.clone(), name));
        }
    }
    Ok(out)
}

/// Write an archive. Timestamps are fixed so the output is reproducible.
/// `password` applies to ZIP only and uses AES-256.
pub fn create(
    paths: &[PathBuf],
    dest: &Path,
    format: Format,
    password: Option<&str>,
) -> Result<usize> {
    let files = collect(paths)?;
    let unwritable = |e: std::io::Error| {
        err(
            ErrorCode::OutputUnwritable,
            format!("{}: {e}", dest.display()),
        )
    };
    let out = BufWriter::new(File::create(dest).map_err(unwritable)?);
    match format {
        Format::Zip => {
            let mut zip = zip::ZipWriter::new(out);
            let opts = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .last_modified_time(zip::DateTime::default());
            for (path, name) in &files {
                let this = match password {
                    Some(p) => opts.with_aes_encryption(zip::AesMode::Aes256, p),
                    None => opts,
                };
                zip.start_file(name.as_str(), this)
                    .map_err(|e| err(ErrorCode::Internal, e.to_string()))?;
                let mut f = File::open(path)?;
                std::io::copy(&mut f, &mut zip).map_err(unwritable)?;
            }
            zip.finish()
                .map_err(|e| err(ErrorCode::Internal, e.to_string()))?;
        }
        Format::Tar => write_tar(out, &files)?,
        Format::TarGz => {
            let gz = flate2::GzBuilder::new()
                .mtime(0)
                .write(out, flate2::Compression::default());
            let gz = write_tar_into(gz, &files)?;
            gz.finish().map_err(unwritable)?;
        }
        Format::TarBz2 => {
            let bz = bzip2::write::BzEncoder::new(out, bzip2::Compression::default());
            let bz = write_tar_into(bz, &files)?;
            bz.finish().map_err(unwritable)?;
        }
        Format::TarXz => {
            let options = lzma_rust2::XzOptions::with_preset(6);
            let xz = lzma_rust2::XzWriter::new(out, options).map_err(unwritable)?;
            let xz = write_tar_into(xz, &files)?;
            xz.finish().map_err(unwritable)?;
        }
        Format::SevenZ => {
            let mut writer = sevenz_rust2::ArchiveWriter::new(out)
                .map_err(|e| err(ErrorCode::Internal, e.to_string()))?;
            if let Some(p) = password {
                writer.set_content_methods(vec![
                    sevenz_rust2::encoder_options::AesEncoderOptions::new(p.into()).into(),
                    sevenz_rust2::EncoderMethod::LZMA2.into(),
                ]);
            }
            for (path, name) in &files {
                let mut entry = sevenz_rust2::ArchiveEntry::new_file(name);
                entry.name = name.clone();
                let source = File::open(path)?;
                writer
                    .push_archive_entry(entry, Some(source))
                    .map_err(|e| err(ErrorCode::Internal, e.to_string()))?;
            }
            writer
                .finish()
                .map_err(|e| err(ErrorCode::Internal, e.to_string()))?;
        }
        Format::TarZst => {
            let tar = write_tar_into(Vec::new(), &files)?;
            ruzstd::encoding::compress(
                tar.as_slice(),
                out,
                ruzstd::encoding::CompressionLevel::Fastest,
            );
        }
        other => {
            return Err(err(
                ErrorCode::InputUnsupported,
                format!("cannot create {other} archives"),
            ));
        }
    }
    Ok(files.len())
}

/// Copy a ZIP, adding files at its root. Existing entries are kept as stored.
pub fn append_zip(source: &Path, dest: &Path, files: &[PathBuf]) -> Result<usize> {
    std::fs::copy(source, dest)?;
    let target = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(dest)?;
    let mut zip = zip::ZipWriter::new_append(target).map_err(|e| corrupt(source, e))?;
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default());
    let list = collect(files)?;
    for (path, name) in &list {
        zip.start_file(name.as_str(), opts)
            .map_err(|e| corrupt(source, e))?;
        let mut f = File::open(path)?;
        std::io::copy(&mut f, &mut zip)?;
    }
    zip.finish().map_err(|e| corrupt(source, e))?;
    Ok(list.len())
}

fn write_tar<W: Write>(w: W, files: &[(PathBuf, String)]) -> Result<()> {
    write_tar_into(w, files).map(|_| ())
}

fn write_tar_into<W: Write>(w: W, files: &[(PathBuf, String)]) -> Result<W> {
    let mut tar = tar::Builder::new(w);
    for (path, name) in files {
        let mut f = File::open(path)?;
        let mut header = tar::Header::new_gnu();
        header.set_size(f.metadata()?.len());
        header.set_mode(0o644);
        header.set_mtime(0);
        header.set_cksum();
        tar.append_data(&mut header, name, &mut f)?;
    }
    Ok(tar.into_inner()?)
}

/// A safe relative path inside the target, or None if the entry escapes it.
fn safe_join(root: &Path, name: &str) -> Option<PathBuf> {
    let rel = Path::new(name);
    let mut out = root.to_path_buf();
    for part in rel.components() {
        match part {
            Component::Normal(p) => out.push(p),
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(out)
}

fn escape_error(name: &str) -> deft_core::error::DeftError {
    err(
        ErrorCode::InputCorrupt,
        format!("unsafe entry {name:?} escapes the target folder"),
    )
}

fn wrong_password(given: bool) -> deft_core::error::DeftError {
    let e = err(
        ErrorCode::InputEncrypted,
        if given {
            "wrong password"
        } else {
            "the archive is password-protected"
        },
    );
    if given {
        e
    } else {
        e.hint("add --password <password>")
    }
}

fn zstd_reader<R: Read>(r: R, path: &Path) -> Result<impl Read> {
    ruzstd::decoding::StreamingDecoder::new(r).map_err(|e| corrupt(path, e))
}

fn sevenz_password(password: Option<&str>) -> sevenz_rust2::Password {
    password.map_or(
        sevenz_rust2::Password::empty(),
        sevenz_rust2::Password::from,
    )
}

fn sevenz_error(path: &Path, e: sevenz_rust2::Error, given: bool) -> deft_core::error::DeftError {
    match e {
        sevenz_rust2::Error::PasswordRequired | sevenz_rust2::Error::MaybeBadPassword(_) => {
            wrong_password(given)
        }
        other => corrupt(path, other),
    }
}

/// Extract into `dir`, enforcing a total size limit against decompression bombs.
pub fn extract(
    path: &Path,
    format: Format,
    dir: &Path,
    limit: u64,
    password: Option<&str>,
) -> Result<usize> {
    let mut total = 0u64;
    let mut count = 0;
    let mut copy = |src: &mut dyn Read, dest: &Path| -> Result<()> {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = File::create(dest)?;
        let budget = limit.saturating_sub(total);
        let n = std::io::copy(&mut src.take(budget + 1), &mut out)?;
        total += n;
        if n > budget {
            return Err(err(
                ErrorCode::LimitExceeded,
                format!("unpacked size exceeds {} bytes", limit),
            ));
        }
        count += 1;
        Ok(())
    };
    let file = BufReader::new(File::open(path)?);
    match format {
        Format::Zip => {
            let mut zip = zip::ZipArchive::new(file).map_err(|e| corrupt(path, e))?;
            for i in 0..zip.len() {
                let mut entry = match password {
                    Some(p) => zip.by_index_decrypt(i, p.as_bytes()),
                    None => zip.by_index(i),
                }
                .map_err(|e| match e {
                    zip::result::ZipError::UnsupportedArchive(m)
                        if m == zip::result::ZipError::PASSWORD_REQUIRED =>
                    {
                        wrong_password(false)
                    }
                    zip::result::ZipError::InvalidPassword => wrong_password(true),
                    other => corrupt(path, other),
                })?;
                let name = entry.name().to_string();
                let dest = safe_join(dir, &name).ok_or_else(|| escape_error(&name))?;
                if entry.is_dir() {
                    std::fs::create_dir_all(&dest)?;
                } else {
                    copy(&mut entry, &dest)?;
                }
            }
        }
        Format::Tar => extract_tar(tar::Archive::new(file), dir, &mut copy, path)?,
        Format::TarGz => extract_tar(
            tar::Archive::new(flate2::read::GzDecoder::new(file)),
            dir,
            &mut copy,
            path,
        )?,
        Format::TarBz2 => extract_tar(
            tar::Archive::new(bzip2::read::BzDecoder::new(file)),
            dir,
            &mut copy,
            path,
        )?,
        Format::TarXz => extract_tar(
            tar::Archive::new(lzma_rust2::XzReader::new(file, true)),
            dir,
            &mut copy,
            path,
        )?,
        Format::TarZst => extract_tar(
            tar::Archive::new(zstd_reader(file, path)?),
            dir,
            &mut copy,
            path,
        )?,
        Format::SevenZ => {
            let mut reader = sevenz_rust2::ArchiveReader::open(path, sevenz_password(password))
                .map_err(|e| sevenz_error(path, e, password.is_some()))?;
            let mut failure = None;
            let done = reader.for_each_entries(|entry, data| {
                let Some(dest) = safe_join(dir, &entry.name) else {
                    failure = Some(escape_error(&entry.name));
                    return Ok(false);
                };
                if entry.is_directory {
                    std::fs::create_dir_all(&dest)?;
                } else if let Err(e) = copy(data, &dest) {
                    failure = Some(e);
                    return Ok(false);
                }
                Ok(true)
            });
            if let Some(e) = failure {
                return Err(e);
            }
            done.map_err(|e| sevenz_error(path, e, password.is_some()))?;
        }
        Format::Gz => {
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "output".into());
            copy(&mut flate2::read::GzDecoder::new(file), &dir.join(stem))?;
        }
        other => {
            return Err(err(
                ErrorCode::InputUnsupported,
                format!("cannot extract {other}"),
            ));
        }
    }
    Ok(count)
}

fn extract_tar<R: Read>(
    mut archive: tar::Archive<R>,
    dir: &Path,
    copy: &mut dyn FnMut(&mut dyn Read, &Path) -> Result<()>,
    path: &Path,
) -> Result<()> {
    for entry in archive.entries().map_err(|e| corrupt(path, e))? {
        let mut entry = entry.map_err(|e| corrupt(path, e))?;
        let name = entry
            .path()
            .map_err(|e| corrupt(path, e))?
            .to_string_lossy()
            .to_string();
        let dest = safe_join(dir, &name).ok_or_else(|| escape_error(&name))?;
        match entry.header().entry_type() {
            tar::EntryType::Directory => std::fs::create_dir_all(&dest)?,
            tar::EntryType::Regular | tar::EntryType::Continuous => copy(&mut entry, &dest)?,
            // Links could point outside the target; skip them.
            _ => {}
        }
    }
    Ok(())
}

/// (name, unpacked size) of each file entry.
pub fn list(path: &Path, format: Format) -> Result<Vec<(String, u64)>> {
    let file = BufReader::new(File::open(path)?);
    fn tar_list<R: Read>(mut a: tar::Archive<R>, path: &Path) -> Result<Vec<(String, u64)>> {
        let mut out = Vec::new();
        for e in a.entries().map_err(|e| corrupt(path, e))? {
            let e = e.map_err(|e| corrupt(path, e))?;
            if e.header().entry_type().is_file() {
                out.push((
                    e.path()
                        .map_err(|e| corrupt(path, e))?
                        .to_string_lossy()
                        .to_string(),
                    e.size(),
                ));
            }
        }
        Ok(out)
    }
    match format {
        Format::Zip => {
            let mut zip = zip::ZipArchive::new(file).map_err(|e| corrupt(path, e))?;
            let mut out = Vec::new();
            for i in 0..zip.len() {
                let e = zip.by_index(i).map_err(|e| corrupt(path, e))?;
                if !e.is_dir() {
                    out.push((e.name().to_string(), e.size()));
                }
            }
            Ok(out)
        }
        Format::Tar => tar_list(tar::Archive::new(file), path),
        Format::TarGz => tar_list(tar::Archive::new(flate2::read::GzDecoder::new(file)), path),
        Format::TarBz2 => tar_list(tar::Archive::new(bzip2::read::BzDecoder::new(file)), path),
        Format::TarXz => tar_list(
            tar::Archive::new(lzma_rust2::XzReader::new(file, true)),
            path,
        ),
        Format::TarZst => tar_list(tar::Archive::new(zstd_reader(file, path)?), path),
        Format::SevenZ => {
            let reader = sevenz_rust2::ArchiveReader::open(path, sevenz_rust2::Password::empty())
                .map_err(|e| sevenz_error(path, e, false))?;
            Ok(reader
                .archive()
                .files
                .iter()
                .filter(|f| !f.is_directory)
                .map(|f| (f.name.clone(), f.size))
                .collect())
        }
        other => Err(err(
            ErrorCode::InputUnsupported,
            format!("cannot list {other}"),
        )),
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
