use crate::model::Circuit;
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};
pub const MAX_FILE_SIZE: u64 = 8 * 1024 * 1024;

pub fn load(path: &Path) -> Result<Circuit, String> {
    let file = fs::File::open(path).map_err(|e| format!("Не удалось открыть: {e}"))?;
    let mut data = Vec::new();
    file.take(MAX_FILE_SIZE + 1)
        .read_to_end(&mut data)
        .map_err(|e| e.to_string())?;
    if data.len() as u64 > MAX_FILE_SIZE {
        return Err("Файл больше 8 МБ".into());
    }
    let circuit: Circuit =
        serde_json::from_slice(&data).map_err(|e| format!("Ошибка JSON: {e}"))?;
    circuit.validate()?;
    Ok(circuit)
}

pub fn save(circuit: &Circuit, path: &Path) -> Result<(), String> {
    circuit.validate()?;
    let data = serde_json::to_vec_pretty(circuit).map_err(|e| e.to_string())?;
    if data.len() as u64 > MAX_FILE_SIZE {
        return Err("Файл больше 8 МБ".into());
    }
    // Create a unique sibling, flush it, then atomically replace the destination.
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let temporary = parent.join(format!(".circuit-{}-{stamp}.tmp", std::process::id()));
    let result = (|| -> std::io::Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(&data)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(&temporary);
        return Err(format!("Не удалось сохранить: {e}"));
    }
    Ok(())
}
