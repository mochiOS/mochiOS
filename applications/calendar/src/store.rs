use crate::date::Date;
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 8] = b"MCAL0001";
const MAX_EVENTS: usize = 10000;
const MAX_FILE: u64 = 32 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub id: u64,
    pub date: Date,
    pub title: String,
    pub time: Option<(u16, u16)>,
    pub location: String,
    pub notes: String,
}
impl Event {
    pub fn validate(&self) -> Result<(), String> {
        if Date::new(self.date.year, self.date.month, self.date.day).is_none() {
            return Err("Enter a valid date (YYYY-MM-DD)".into());
        }
        if self.title.trim().is_empty() || self.title.len() > 256 {
            return Err("Enter a title, or shorten it if it is too long".into());
        }
        if self.location.len() > 1024 || self.notes.len() > 16384 {
            return Err("The location or notes are too long".into());
        }
        if self
            .time
            .is_some_and(|(start, end)| start >= end || end >= 1440)
        {
            return Err("The end time must be later than the start time on the same day".into());
        }
        Ok(())
    }
    pub fn sort_key(&self) -> (Date, Option<u16>, u64) {
        (self.date, self.time.map(|t| t.0), self.id)
    }
}

pub struct Store {
    path: PathBuf,
    baseline: Vec<u8>,
    present: bool,
    pub events: Vec<Event>,
}
impl Store {
    pub fn load(path: PathBuf) -> Result<Self, String> {
        let parent = path.parent().ok_or("Invalid calendar storage path")?;
        fs::create_dir_all(parent).map_err(|e| format!("Cannot open calendar storage: {e}"))?;
        let lock = path.with_extension("lock");
        fs::create_dir(&lock)
            .map_err(|e| format!("Calendar storage is busy or unavailable: {e}"))?;
        let _lock = Lock(lock);
        recover(&path).map_err(|e| format!("Cannot recover calendar: {e}"))?;
        let baseline = read(&path).map_err(|e| format!("Cannot read calendar: {e}"))?;
        let present = path.try_exists().map_err(|e| e.to_string())?;
        let events = if !present { vec![] } else { decode(&baseline)? };
        Ok(Self {
            path,
            baseline,
            present,
            events,
        })
    }
    pub fn save(&mut self, mut events: Vec<Event>) -> Result<(), String> {
        let bytes = encode(&events)?;
        let parent = self.path.parent().ok_or("Invalid calendar storage path")?;
        fs::create_dir_all(parent).map_err(|e| format!("Cannot create calendar storage: {e}"))?;
        let lock = self.path.with_extension("lock");
        fs::create_dir(&lock)
            .map_err(|e| format!("Calendar storage is busy or unavailable: {e}"))?;
        let _lock = Lock(lock);
        recover(&self.path).map_err(|e| e.to_string())?;
        if self.path.try_exists().map_err(|e| e.to_string())? != self.present
            || read(&self.path).map_err(|e| e.to_string())? != self.baseline
        {
            return Err("Calendar changed in another window. Reopen Calendar before saving; your edit has been kept.".into());
        }
        let temporary = self.path.with_extension("new");
        let backup = self.path.with_extension("previous");
        // A leftover temporary file is never the committed calendar.
        remove_if_present(&temporary).map_err(|e| e.to_string())?;
        let result = (|| -> io::Result<()> {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary)?;
            file.write_all(&bytes)?;
            file.flush()?;
            file.sync_all()?;
            drop(file);
            let original = match fs::rename(&self.path, &backup) {
                Ok(()) => true,
                Err(e) if e.kind() == io::ErrorKind::NotFound => false,
                Err(e) => return Err(e),
            };
            if let Err(error) = fs::rename(&temporary, &self.path) {
                if original {
                    fs::rename(&backup, &self.path)?;
                }
                return Err(error);
            }
            // The new file is committed; leftover backups are recovered on the next save.
            let _ = remove_if_present(&backup);
            Ok(())
        })();
        if let Err(error) = result {
            let _ = remove_if_present(&temporary);
            return Err(format!("Could not save calendar: {error}"));
        }
        events.sort_by_key(Event::sort_key);
        self.events = events;
        self.baseline = bytes;
        self.present = true;
        Ok(())
    }
    pub fn on_date(&self, date: Date) -> &[Event] {
        let first = self.events.partition_point(|event| event.date < date);
        let last = self.events.partition_point(|event| event.date <= date);
        &self.events[first..last]
    }
    pub fn next_id(&self) -> Result<u64, String> {
        self.events
            .iter()
            .map(|e| e.id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| "Calendar is full".into())
    }
}
struct Lock(PathBuf);
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = fs::remove_dir(&self.0);
    }
}
fn read(path: &Path) -> io::Result<Vec<u8>> {
    match fs::File::open(path) {
        Ok(file) => {
            let mut bytes = Vec::new();
            file.take(MAX_FILE + 1).read_to_end(&mut bytes)?;
            if bytes.len() as u64 > MAX_FILE {
                return Err(io::Error::other("Calendar exceeds the storage limit"));
            }
            Ok(bytes)
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(vec![]),
        Err(e) => Err(e),
    }
}
fn recover(path: &Path) -> io::Result<()> {
    let backup = path.with_extension("previous");
    if backup.try_exists()? {
        if path.try_exists()? {
            let current = read(path)?;
            decode(&current).map_err(io::Error::other)?;
            remove_if_present(&backup)
        } else {
            fs::rename(backup, path)
        }
    } else {
        Ok(())
    }
}
fn remove_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

fn encode(events: &[Event]) -> Result<Vec<u8>, String> {
    if events.len() > MAX_EVENTS {
        return Err("Too many events".into());
    }
    let mut ids = HashSet::new();
    let mut out = MAGIC.to_vec();
    out.extend_from_slice(&(events.len() as u32).to_le_bytes());
    for event in events {
        event.validate()?;
        if event.id == 0 || !ids.insert(event.id) {
            return Err("Invalid event identifier".into());
        }
        out.extend_from_slice(&event.id.to_le_bytes());
        out.extend_from_slice(&event.date.year.to_le_bytes());
        out.extend_from_slice(&[event.date.month, event.date.day]);
        let (start, end) = event.time.unwrap_or((u16::MAX, u16::MAX));
        out.extend_from_slice(&start.to_le_bytes());
        out.extend_from_slice(&end.to_le_bytes());
        for text in [&event.title, &event.location, &event.notes] {
            out.extend_from_slice(&(text.len() as u32).to_le_bytes());
            out.extend_from_slice(text.as_bytes());
        }
        if out.len() as u64 > MAX_FILE {
            return Err("Calendar exceeds the storage limit".into());
        }
    }
    Ok(out)
}
fn decode(bytes: &[u8]) -> Result<Vec<Event>, String> {
    let invalid = || {
        "Calendar data is damaged or uses an unsupported version. The file has been left untouched."
            .to_owned()
    };
    if bytes.len() as u64 > MAX_FILE || !bytes.starts_with(MAGIC) {
        return Err(invalid());
    }
    let mut input = &bytes[8..];
    fn take<const N: usize>(input: &mut &[u8]) -> Option<[u8; N]> {
        let value = input.get(..N)?.try_into().ok()?;
        *input = &input[N..];
        Some(value)
    }
    let count = u32::from_le_bytes(take(&mut input).ok_or_else(invalid)?) as usize;
    if count > MAX_EVENTS {
        return Err(invalid());
    }
    let mut events = Vec::with_capacity(count);
    for _ in 0..count {
        let id = u64::from_le_bytes(take(&mut input).ok_or_else(invalid)?);
        let year = i32::from_le_bytes(take(&mut input).ok_or_else(invalid)?);
        let [month, day] = take(&mut input).ok_or_else(invalid)?;
        let date = Date::new(year, month, day).ok_or_else(invalid)?;
        let start = u16::from_le_bytes(take(&mut input).ok_or_else(invalid)?);
        let end = u16::from_le_bytes(take(&mut input).ok_or_else(invalid)?);
        let mut text = Vec::new();
        for limit in [256, 1024, 16384] {
            let length = u32::from_le_bytes(take(&mut input).ok_or_else(invalid)?) as usize;
            if length > limit {
                return Err(invalid());
            }
            let value = std::str::from_utf8(input.get(..length).ok_or_else(invalid)?)
                .map_err(|_| invalid())?
                .to_owned();
            input = &input[length..];
            text.push(value);
        }
        events.push(Event {
            id,
            date,
            time: if start == u16::MAX && end == u16::MAX {
                None
            } else {
                Some((start, end))
            },
            title: text.remove(0),
            location: text.remove(0),
            notes: text.remove(0),
        });
    }
    if !input.is_empty() {
        return Err(invalid());
    }
    encode(&events).map_err(|_| invalid())?;
    events.sort_by_key(Event::sort_key);
    Ok(events)
}

pub fn storage_path() -> Result<PathBuf, String> {
    #[cfg(target_os = "linux")]
    {
        if let Some(root) = std::env::var_os("MOCHIOS_PREVIEW_CONFIG_ROOT") {
            return Ok(PathBuf::from(root).join("calendar/events-v1"));
        }
        let home = std::env::var_os("HOME").ok_or("No home directory is available")?;
        return Ok(PathBuf::from(home).join(".local/share/mochios/calendar/events-v1"));
    }
    #[cfg(not(target_os = "linux"))]
    {
        let user = std::env::var("USER").unwrap_or_else(|_| "root".into());
        if user.is_empty()
            || user.len() > 64
            || !user
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return Err("Invalid user name".into());
        }
        Ok(
            Path::new("/libraries/applications/org.mochios.calendar/users")
                .join(user)
                .join("events-v1"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "calendar-store-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn path(&self) -> PathBuf {
            self.0.join("events")
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn event(id: u64) -> Event {
        Event {
            id,
            date: Date::parse("2026-10-23").unwrap(),
            title: "打ち合わせ".into(),
            time: Some((540, 600)),
            location: "会議室".into(),
            notes: "notes\nsecond line".into(),
        }
    }
    #[test]
    fn events_survive_create_update_delete_and_reload() {
        let dir = Directory::new();
        let mut store = Store::load(dir.path()).unwrap();
        store.save(vec![event(1)]).unwrap();
        let mut store = Store::load(dir.path()).unwrap();
        assert_eq!(store.events, vec![event(1)]);
        let mut edited = event(1);
        edited.title = "Changed".into();
        edited.time = None;
        store.save(vec![edited.clone()]).unwrap();
        assert_eq!(Store::load(dir.path()).unwrap().events, vec![edited]);
        store.save(vec![]).unwrap();
        assert!(Store::load(dir.path()).unwrap().events.is_empty());
    }
    #[test]
    fn stale_windows_cannot_silently_replace_newer_events() {
        let dir = Directory::new();
        let mut first = Store::load(dir.path()).unwrap();
        let mut second = Store::load(dir.path()).unwrap();
        first.save(vec![event(1)]).unwrap();
        assert!(second.save(vec![event(2)]).is_err());
        assert_eq!(Store::load(dir.path()).unwrap().events, vec![event(1)]);
    }
    #[test]
    fn validation_and_corrupt_storage_never_replace_committed_data() {
        let dir = Directory::new();
        let mut store = Store::load(dir.path()).unwrap();
        store.save(vec![event(1)]).unwrap();
        let original = fs::read(dir.path()).unwrap();
        let mut bad = event(2);
        bad.time = Some((700, 600));
        assert!(store.save(vec![bad]).is_err());
        assert_eq!(fs::read(dir.path()).unwrap(), original);
        for n in 0..original.len() {
            assert!(decode(&original[..n]).is_err());
        }
        let mut extra = original.clone();
        extra.push(0);
        assert!(decode(&extra).is_err());
        fs::write(dir.path(), b"").unwrap();
        assert!(Store::load(dir.path()).is_err());
        fs::write(dir.path(), b"broken").unwrap();
        assert!(Store::load(dir.path()).is_err());
        assert_eq!(fs::read(dir.path()).unwrap(), b"broken");
    }
    #[test]
    fn recovers_an_interrupted_publish_and_refuses_busy_storage() {
        let dir = Directory::new();
        let mut store = Store::load(dir.path()).unwrap();
        store.save(vec![event(1)]).unwrap();
        fs::rename(dir.path(), dir.path().with_extension("previous")).unwrap();
        assert_eq!(Store::load(dir.path()).unwrap().events, vec![event(1)]);
        fs::create_dir(dir.path().with_extension("lock")).unwrap();
        assert!(store.save(vec![]).is_err());
        fs::remove_dir(dir.path().with_extension("lock")).unwrap();
        assert_eq!(Store::load(dir.path()).unwrap().events, vec![event(1)]);
    }
}
