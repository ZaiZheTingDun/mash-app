//! Small, consistent JSON persistence primitives.
//!
//! Missing files are treated as first-run state. Existing files that cannot be
//! read or decoded are errors: silently replacing them with defaults would make
//! the next successful save destroy recoverable user data.

use serde::{de::DeserializeOwned, Serialize};
use std::fs;
use std::io::{self, Write};
use std::path::Path;

pub(crate) fn read_json_or_default<T>(path: &Path, label: &str) -> Result<T, String>
where
    T: DeserializeOwned + Default,
{
    match fs::read_to_string(path) {
        Ok(contents) => serde_json::from_str(&contents)
            .map_err(|error| format!("{label}格式错误（{}）：{error}", path.display())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(T::default()),
        Err(error) => Err(format!("读取{label}失败（{}）：{error}", path.display())),
    }
}

pub(crate) fn write_json_atomic<T>(path: &Path, value: &T, label: &str) -> Result<(), String>
where
    T: Serialize + ?Sized,
{
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if parent != Path::new(".") {
        fs::create_dir_all(parent)
            .map_err(|error| format!("创建{label}目录失败（{}）：{error}", parent.display()))?;
    }

    let json =
        serde_json::to_vec_pretty(value).map_err(|error| format!("序列化{label}失败：{error}"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| format!("创建{label}临时文件失败（{}）：{error}", parent.display()))?;
    temporary
        .write_all(&json)
        .map_err(|error| format!("写入{label}临时文件失败：{error}"))?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| format!("同步{label}临时文件失败：{error}"))?;
    temporary
        .persist(path)
        .map_err(|error| format!("替换{label}失败（{}）：{}", path.display(), error.error))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
    struct Fixture {
        value: u8,
    }

    #[test]
    fn missing_json_uses_default() {
        let dir = tempfile::tempdir().unwrap();
        let value =
            read_json_or_default::<Fixture>(&dir.path().join("missing.json"), "测试配置").unwrap();
        assert_eq!(value, Fixture::default());
    }

    #[test]
    fn malformed_json_is_not_silently_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.json");
        fs::write(&path, "not json").unwrap();

        let error = read_json_or_default::<Fixture>(&path, "测试配置").unwrap_err();

        assert!(error.contains("测试配置格式错误"));
        assert_eq!(fs::read_to_string(path).unwrap(), "not json");
    }

    #[test]
    fn atomic_write_round_trips_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("fixture.json");
        let expected = Fixture { value: 7 };

        write_json_atomic(&path, &expected, "测试配置").unwrap();

        assert_eq!(
            read_json_or_default::<Fixture>(&path, "测试配置").unwrap(),
            expected
        );
    }
}
