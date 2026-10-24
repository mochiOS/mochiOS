use crate::{
    engine::{self, Mode, Settings, Value},
    preferences, units,
};
use std::collections::HashSet;
use std::path::Path;
pub const LIMIT: usize = 200;
#[derive(Clone)]
pub struct History {
    pub id: u64,
    pub input: String,
    pub result: Value,
    pub settings: Settings,
    pub units: (usize, usize, usize),
}
fn file(path: &Path) -> std::path::PathBuf {
    path.with_file_name("history-v1")
}
pub fn load(path: &Path) -> Result<Vec<History>, String> {
    let Some(bytes) = preferences::read_data(&file(path), 512 * 1024)? else {
        return Ok(vec![]);
    };
    decode(&bytes)
}
pub fn save(path: &Path, entries: &[History]) -> Result<(), String> {
    preferences::write_data(&file(path), &encode(entries)?)
}
fn encode(entries: &[History]) -> Result<Vec<u8>, String> {
    if entries.len() > LIMIT {
        return Err("Too many history entries".into());
    }
    let mut out = b"MCALCH01".to_vec();
    out.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    for e in entries {
        if e.input.len() > 2048 {
            return Err("The expression is too long to save in history".into());
        }
        out.extend_from_slice(&e.id.to_le_bytes());
        out.extend_from_slice(&[
            match e.settings.mode {
                Mode::Standard => 0,
                Mode::Scientific => 1,
                Mode::Convert => 2,
                Mode::Programmer => 3,
            },
            u8::from(e.settings.degrees),
            e.settings.base as u8,
            e.settings.bits as u8,
            u8::from(e.settings.signed),
            e.units.0 as u8,
            e.units.1 as u8,
            e.units.2 as u8,
        ]);
        out.extend_from_slice(&(e.input.len() as u32).to_le_bytes());
        out.extend_from_slice(e.input.as_bytes());
    }
    Ok(out)
}
fn decode(bytes: &[u8]) -> Result<Vec<History>, String> {
    let bad = || {
        "Calculation history is damaged or uses an unsupported format; it has been left untouched"
            .to_owned()
    };
    if !bytes.starts_with(b"MCALCH01") {
        return Err(bad());
    }
    let mut data = &bytes[8..];
    fn take<const N: usize>(data: &mut &[u8]) -> Option<[u8; N]> {
        let raw = data.get(..N)?.try_into().ok()?;
        *data = &data[N..];
        Some(raw)
    }
    let count = u32::from_le_bytes(take(&mut data).ok_or_else(bad)?) as usize;
    if count > LIMIT {
        return Err(bad());
    }
    let mut entries = Vec::with_capacity(count);
    let mut ids = HashSet::new();
    for _ in 0..count {
        let id = u64::from_le_bytes(take(&mut data).ok_or_else(bad)?);
        if !ids.insert(id) {
            return Err(bad());
        }
        let [mode, degrees, base, bits, signed, category, from, to] =
            take(&mut data).ok_or_else(bad)?;
        if degrees > 1 || signed > 1 {
            return Err(bad());
        }
        let settings = Settings {
            mode: *Mode::ALL.get(mode as usize).ok_or_else(bad)?,
            degrees: degrees != 0,
            base: base as u32,
            bits: bits as u32,
            signed: signed != 0,
        };
        let category = category as usize;
        let from = from as usize;
        let to = to as usize;
        let units = units::CATEGORIES.get(category).ok_or_else(bad)?;
        if from >= units.units.len() || to >= units.units.len() {
            return Err(bad());
        }
        let len = u32::from_le_bytes(take(&mut data).ok_or_else(bad)?) as usize;
        if len > 2048 {
            return Err(bad());
        }
        let input = std::str::from_utf8(data.get(..len).ok_or_else(bad)?)
            .map_err(|_| bad())?
            .to_owned();
        data = &data[len..];
        let mut result = engine::evaluate(&input, settings, None).map_err(|_| bad())?;
        if settings.mode == Mode::Convert {
            let Value::Real(value) = result else {
                return Err(bad());
            };
            result =
                Value::Real(crate::units::convert(value, category, from, to).map_err(|_| bad())?);
        }
        entries.push(History {
            id,
            input,
            result,
            settings,
            units: (category, from, to),
        });
    }
    if !data.is_empty() {
        return Err(bad());
    }
    Ok(entries)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history_round_trips_and_rejects_malformed_data() {
        let settings = Settings::default();
        let e = History {
            id: 1,
            input: "(1/3)*3".into(),
            result: Value::Real(crate::number::Number::ONE),
            settings,
            units: (0, 0, 1),
        };
        let bytes = encode(&[e]).unwrap();
        let entries = decode(&bytes).unwrap();
        assert_eq!(entries[0].result, Value::Real(crate::number::Number::ONE));
        for i in 0..bytes.len() {
            assert!(decode(&bytes[..i]).is_err());
        }
        let mut extra = bytes;
        extra.push(0);
        assert!(decode(&extra).is_err());
    }
}
