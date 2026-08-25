use std::path::Path;

const DEVICE_NAME_FILE: &str = "device.name";

pub fn load_device_name(path: &Path) -> Option<String> {
    let name = std::fs::read_to_string(path.join(DEVICE_NAME_FILE)).ok()?;
    let name = name.trim().to_string();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

pub fn persist_device_name(path: &Path, name: &str) {
    let _ = std::fs::write(path.join(DEVICE_NAME_FILE), name);
}
