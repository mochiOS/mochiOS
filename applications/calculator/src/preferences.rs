use crate::engine::Mode;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

pub fn path() -> Result<PathBuf, String> {
    #[cfg(target_os = "linux")]
    {
        if let Some(root) = std::env::var_os("MOCHIOS_PREVIEW_CONFIG_ROOT") {
            return Ok(PathBuf::from(root).join("calculator/start-mode"));
        }
        let home = std::env::var_os("HOME").ok_or("No home directory is available")?;
        Ok(PathBuf::from(home).join(".local/share/mochios/calculator/start-mode"))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let user = std::env::var("USER").unwrap_or_else(|_| "root".into());
        if user.is_empty()
            || user.len() > 64
            || !user
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        {
            return Err("Invalid user name".into());
        }
        Ok(
            Path::new("/libraries/applications/org.mochios.calculator/users")
                .join(user)
                .join("start-mode"),
        )
    }
}
pub fn load(path: &Path) -> Result<Mode, String> {
    match read_data(path, 64)? {
        None => Ok(Mode::Standard),
        Some(bytes) => std::str::from_utf8(&bytes)
            .ok()
            .and_then(|text| Mode::parse(text.trim()))
            .ok_or_else(|| "The saved start mode is invalid; Standard mode is in use".into()),
    }
}
pub fn read_data(path: &Path, limit: u64) -> Result<Option<Vec<u8>>, String> {
    let parent = path.parent().ok_or("Invalid storage path")?;
    if !parent.try_exists().map_err(|e| e.to_string())? {
        return Ok(None);
    }
    let lock = path.with_extension("lock");
    fs::create_dir(&lock).map_err(|e| format!("Storage is busy or unavailable: {e}"))?;
    let _lock = Lock(lock);
    recover(path).map_err(|e| e.to_string())?;
    match fs::File::open(path) {
        Ok(file) => {
            let mut bytes = Vec::new();
            file.take(limit + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            if bytes.len() as u64 > limit {
                return Err("Stored data exceeds its size limit".into());
            }
            Ok(Some(bytes))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}
pub fn save(path: &Path, mode: Mode) -> Result<(), String> {
    write_data(path, format!("{}\n", mode.label()).as_bytes())
}
pub fn write_data(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("Invalid storage path")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let lock = path.with_extension("lock");
    fs::create_dir(&lock).map_err(|e| format!("Storage is busy or unavailable: {e}"))?;
    let _lock = Lock(lock);
    recover(path).map_err(|e| e.to_string())?;
    let temporary = path.with_extension("new");
    let backup = path.with_extension("previous");
    let result = (|| -> io::Result<()> {
        remove(&temporary)?;
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        let previous = match fs::rename(path, &backup) {
            Ok(()) => true,
            Err(e) if e.kind() == io::ErrorKind::NotFound => false,
            Err(e) => return Err(e),
        };
        if let Err(e) = fs::rename(&temporary, path) {
            if previous {
                fs::rename(&backup, path)?;
            }
            return Err(e);
        }
        let _ = remove(&backup);
        Ok(())
    })();
    if let Err(e) = result {
        let _ = remove(&temporary);
        return Err(format!("Cannot save calculator data: {e}"));
    }
    Ok(())
}
struct Lock(PathBuf);
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = fs::remove_dir(&self.0);
    }
}
fn remove(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}
fn recover(path: &Path) -> io::Result<()> {
    let previous = path.with_extension("previous");
    if previous.try_exists()? {
        if path.try_exists()? {
            remove(&previous)
        } else {
            fs::rename(previous, path)
        }
    } else {
        Ok(())
    }
}

pub fn copy(value: &str) -> Result<(), String> {
    #[cfg(target_os = "mochios")]
    {
        appcore::clipboard::set_text(value).map_err(|e| format!("Cannot copy the result: {e:?}"))
    }
    #[cfg(not(target_os = "mochios"))]
    {
        let root = std::env::var_os("MOCHIOS_PREVIEW_CONFIG_ROOT")
            .ok_or("Clipboard access requires the desktop preview")?;
        fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        fs::write(PathBuf::from(root).join("clipboard.txt"), value).map_err(|e| e.to_string())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn start_mode_is_explicit_and_survives_restart() {
        let dir = std::env::temp_dir().join(format!("calculator-prefs-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir(&dir).unwrap();
        let path = dir.join("mode");
        assert_eq!(load(&path).unwrap(), Mode::Standard);
        for mode in Mode::ALL {
            save(&path, mode).unwrap();
            assert_eq!(load(&path).unwrap(), mode);
        }
        fs::create_dir(path.with_extension("lock")).unwrap();
        assert!(save(&path, Mode::Scientific).is_err());
        fs::remove_dir(path.with_extension("lock")).unwrap();
        assert_eq!(load(&path).unwrap(), Mode::Programmer);
        fs::remove_dir_all(dir).unwrap();
    }
}
