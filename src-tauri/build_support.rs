use std::fs;
use std::io;
use std::path::Path;

/// Stage only the lightweight Python package. Native dependencies and OCR
/// models belong to the separately installed runtime.
pub fn stage_cv_code(source: &Path, destination: &Path) -> io::Result<()> {
    if !source.join("__init__.py").is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "missing mash_cv package",
        ));
    }
    if destination.exists() {
        fs::remove_dir_all(destination)?;
    }
    copy_package(source, destination)
}

fn copy_package(source: &Path, destination: &Path) -> io::Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let name = entry.file_name();
        let path = entry.path();
        if name == "models" || name == "__pycache__" {
            continue;
        }
        if path
            .extension()
            .is_some_and(|ext| ext == "pyc" || ext == "pyo")
        {
            continue;
        }
        let target = destination.join(name);
        if entry.file_type()?.is_dir() {
            copy_package(&path, &target)?;
        } else {
            let mut contents = fs::read(&path)?;
            if path
                .extension()
                .is_some_and(|ext| ext == "py" || ext == "json" || ext == "txt")
            {
                let mut normalized = Vec::with_capacity(contents.len());
                let mut bytes = contents.iter().peekable();
                while let Some(&byte) = bytes.next() {
                    if byte == b'\r' && bytes.peek() == Some(&&b'\n') {
                        continue;
                    }
                    normalized.push(byte);
                }
                contents = normalized;
            }
            fs::write(target, contents)?;
        }
    }
    Ok(())
}
