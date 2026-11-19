use ferrisync_core::api::{
    self, ApiState, ConflictContents, DeviceCleanup, DeviceEntry, DiscoveredDevice,
    FileHistoryEntry, FolderEntry, FolderPeer, PendingFolderPairing, SessionEntry, SharedFolder,
};
use ferrisync_core::health::{DeviceStatus, FolderStatus, HealthSummary};
use ferrisync_core::sync_engine::conflicts::ConflictEntry;
use ferrisync_core::sync_engine::session::SyncResult;
use ferrisync_core::sync_engine::SyncEvent;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyList};
use pyo3::BoundObject;
use std::path::PathBuf;
use std::sync::OnceLock;

static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

fn runtime() -> PyResult<&'static tokio::runtime::Runtime> {
    if let Some(rt) = RUNTIME.get() {
        return Ok(rt);
    }
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("ferrisync-python")
        .build()
        .map_err(|e| PyRuntimeError::new_err(format!("failed to start Tokio runtime: {e}")))?;
    let _ = RUNTIME.set(rt);
    Ok(RUNTIME.get().expect("Tokio runtime was just initialized"))
}

fn block_on<F, T, E>(py: Python<'_>, fut: F) -> PyResult<T>
where
    F: std::future::Future<Output = Result<T, E>> + Send,
    T: Send,
    E: std::fmt::Display + Send + 'static,
{
    let rt = runtime()?;
    #[allow(deprecated)]
    py.allow_threads(move || {
        rt.block_on(fut)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    })
}

fn anyhow_block_on<F, T>(py: Python<'_>, fut: F) -> PyResult<T>
where
    F: std::future::Future<Output = anyhow::Result<T>> + Send,
    T: Send,
{
    block_on(py, fut)
}

fn default_data_dir() -> PyResult<String> {
    let base = dirs_path().ok_or_else(|| {
        PyRuntimeError::new_err(
            "could not determine a default data directory; pass data_dir explicitly",
        )
    })?;
    Ok(base.join("ferrisync").to_string_lossy().to_string())
}

fn dirs_path() -> Option<PathBuf> {
    // Match the CLI: dirs::data_dir()/ferrisync on supported platforms.
    #[allow(unused_imports)]
    use std::env;
    #[cfg(target_os = "linux")]
    {
        if let Ok(xdg) = env::var("XDG_DATA_HOME") {
            if !xdg.is_empty() {
                return Some(PathBuf::from(xdg));
            }
        }
        if let Ok(home) = env::var("HOME") {
            return Some(PathBuf::from(home).join(".local/share"));
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = env::var("HOME") {
            return Some(PathBuf::from(home).join("Library/Application Support"));
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = env::var("APPDATA") {
            return Some(PathBuf::from(appdata));
        }
    }
    env::var("HOME").ok().map(PathBuf::from)
}

fn sync_event_to_py(py: Python<'_>, event: SyncEvent) -> PyResult<Py<PyAny>> {
    let d = PyDict::new(py);
    match event {
        SyncEvent::Syncing { folder_id } => {
            d.set_item("type", "syncing")?;
            d.set_item("folder_id", folder_id)?;
        }
        SyncEvent::Idle => {
            d.set_item("type", "idle")?;
        }
        SyncEvent::Error { message } => {
            d.set_item("type", "error")?;
            d.set_item("message", message)?;
        }
        SyncEvent::PairRequested { name, id } => {
            d.set_item("type", "pair_requested")?;
            d.set_item("name", name)?;
            d.set_item("id", id)?;
        }
        SyncEvent::FolderPairRequested { name, id, folder } => {
            d.set_item("type", "folder_pair_requested")?;
            d.set_item("name", name)?;
            d.set_item("id", id)?;
            d.set_item("folder", folder)?;
        }
        SyncEvent::DevicePaired { name, id } => {
            d.set_item("type", "device_paired")?;
            d.set_item("name", name)?;
            d.set_item("id", id)?;
        }
        SyncEvent::FilePulled { path, device } => {
            d.set_item("type", "file_pulled")?;
            d.set_item("path", path)?;
            d.set_item("device", device)?;
        }
        SyncEvent::FilePushed { path, device } => {
            d.set_item("type", "file_pushed")?;
            d.set_item("path", path)?;
            d.set_item("device", device)?;
        }
        SyncEvent::Conflict {
            path,
            winner,
            loser,
        } => {
            d.set_item("type", "conflict")?;
            d.set_item("path", path)?;
            d.set_item("winner", winner)?;
            d.set_item("loser", loser)?;
        }
        SyncEvent::Progress {
            folder_id,
            stage,
            files_done,
            files_total,
            bytes_done,
            bytes_total,
        } => {
            d.set_item("type", "progress")?;
            d.set_item("folder_id", folder_id)?;
            d.set_item("stage", stage)?;
            d.set_item("files_done", files_done)?;
            d.set_item("files_total", files_total)?;
            d.set_item("bytes_done", bytes_done)?;
            d.set_item("bytes_total", bytes_total)?;
        }
    }
    Ok(d.into_any().unbind())
}

fn sync_result_to_py(py: Python<'_>, r: SyncResult) -> PyResult<Py<PyAny>> {
    let d = PyDict::new(py);
    d.set_item("pulled", r.pulled)?;
    d.set_item("pushed", r.pushed)?;
    d.set_item("conflicts", r.conflicts)?;
    d.set_item("pulled_bytes", r.pulled_bytes)?;
    d.set_item("pushed_bytes", r.pushed_bytes)?;
    Ok(d.into_any().unbind())
}

fn discovered_to_py(py: Python<'_>, d: DiscoveredDevice) -> PyResult<Py<PyAny>> {
    let o = PyDict::new(py);
    o.set_item("id", d.id)?;
    o.set_item("name", d.name)?;
    o.set_item("ip", d.ip)?;
    o.set_item("port", d.port)?;
    Ok(o.into_any().unbind())
}

fn device_entry_to_py(py: Python<'_>, d: DeviceEntry) -> PyResult<Py<PyAny>> {
    let o = PyDict::new(py);
    o.set_item("id", d.id)?;
    o.set_item("name", d.name)?;
    o.set_item("last_seen", d.last_seen)?;
    Ok(o.into_any().unbind())
}

fn folder_peer_to_py(py: Python<'_>, p: FolderPeer) -> PyResult<Py<PyAny>> {
    let o = PyDict::new(py);
    o.set_item("device_id", p.device_id)?;
    o.set_item("mode", p.mode)?;
    o.set_item("remote_path", p.remote_path)?;
    o.set_item("enabled", p.enabled)?;
    Ok(o.into_any().unbind())
}

fn folder_entry_to_py(py: Python<'_>, f: FolderEntry) -> PyResult<Py<PyAny>> {
    let peers = PyList::empty(py);
    for p in f.peers {
        peers.append(folder_peer_to_py(py, p)?)?;
    }
    let o = PyDict::new(py);
    o.set_item("id", f.id)?;
    o.set_item("local_path", f.local_path)?;
    o.set_item("name", f.name)?;
    o.set_item("guid", f.guid)?;
    o.set_item("device_id", f.device_id)?;
    o.set_item("direction", f.direction)?;
    o.set_item("peers", peers)?;
    o.set_item("last_sync_at", f.last_sync_at)?;
    Ok(o.into_any().unbind())
}

fn session_entry_to_py(py: Python<'_>, s: SessionEntry) -> PyResult<Py<PyAny>> {
    let o = PyDict::new(py);
    o.set_item("ts", s.ts)?;
    o.set_item("direction", s.direction)?;
    o.set_item("peer_device", s.peer_device)?;
    o.set_item("addr", s.addr)?;
    o.set_item("folder_path", s.folder_path)?;
    o.set_item("status", s.status)?;
    o.set_item("pushed_count", s.pushed_count)?;
    o.set_item("pulled_count", s.pulled_count)?;
    o.set_item("conflicts_count", s.conflicts_count)?;
    o.set_item("pushed_bytes", s.pushed_bytes)?;
    o.set_item("pulled_bytes", s.pulled_bytes)?;
    Ok(o.into_any().unbind())
}

fn conflict_entry_to_py(py: Python<'_>, c: ConflictEntry) -> PyResult<Py<PyAny>> {
    let o = PyDict::new(py);
    o.set_item("folder_id", c.folder_id)?;
    o.set_item("path", c.path)?;
    o.set_item("backup_path", c.backup_path)?;
    o.set_item("loser_label", c.loser_label)?;
    o.set_item("winner_mtime_secs", c.winner_mtime_secs)?;
    o.set_item("winner_size", c.winner_size)?;
    o.set_item("loser_mtime_secs", c.loser_mtime_secs)?;
    o.set_item("loser_size", c.loser_size)?;
    Ok(o.into_any().unbind())
}

fn conflict_contents_to_py(py: Python<'_>, c: ConflictContents) -> PyResult<Py<PyAny>> {
    let o = PyDict::new(py);
    o.set_item("winner", c.winner)?;
    o.set_item("loser", c.loser)?;
    o.set_item("winner_truncated", c.winner_truncated)?;
    o.set_item("loser_truncated", c.loser_truncated)?;
    o.set_item("textual", c.textual)?;
    Ok(o.into_any().unbind())
}

fn shared_folder_to_py(py: Python<'_>, s: SharedFolder) -> PyResult<Py<PyAny>> {
    let o = PyDict::new(py);
    o.set_item("id", s.id)?;
    o.set_item("folder_guid", s.folder_guid)?;
    o.set_item("name", s.name)?;
    o.set_item("local_path", s.local_path)?;
    o.set_item("discoverable", s.discoverable)?;
    o.set_item("enabled", s.enabled)?;
    o.set_item("permissions", s.permissions)?;
    Ok(o.into_any().unbind())
}

fn pending_folder_pairing_to_py(py: Python<'_>, p: PendingFolderPairing) -> PyResult<Py<PyAny>> {
    let o = PyDict::new(py);
    o.set_item("device_id", p.device_id)?;
    o.set_item("device_name", p.device_name)?;
    o.set_item("folder_guid", p.folder_guid)?;
    o.set_item("folder_name", p.folder_name)?;
    Ok(o.into_any().unbind())
}

fn device_cleanup_to_py(py: Python<'_>, c: DeviceCleanup) -> PyResult<Py<PyAny>> {
    let o = PyDict::new(py);
    o.set_item("sessions_removed", c.sessions_removed)?;
    o.set_item("history_removed", c.history_removed)?;
    o.set_item("metadata_removed", c.metadata_removed)?;
    o.set_item("folders_removed", c.folders_removed)?;
    o.set_item("device_removed", c.device_removed)?;
    Ok(o.into_any().unbind())
}

fn file_history_to_py(py: Python<'_>, h: FileHistoryEntry) -> PyResult<Py<PyAny>> {
    let o = PyDict::new(py);
    o.set_item("path", h.path)?;
    o.set_item("device_id", h.device_id)?;
    o.set_item("action", h.action)?;
    o.set_item("size", h.size)?;
    o.set_item("recorded_at", h.recorded_at)?;
    Ok(o.into_any().unbind())
}

fn to_py_obj<'py, T>(py: Python<'py>, v: T) -> PyResult<Py<PyAny>>
where
    T: IntoPyObject<'py>,
{
    Ok(v.into_pyobject(py)
        .map_err(|_| PyRuntimeError::new_err("failed to convert value to Python"))?
        .into_any()
        .unbind())
}

fn json_value_to_py(py: Python<'_>, v: &serde_json::Value) -> PyResult<Py<PyAny>> {
    match v {
        serde_json::Value::Null => Ok(py.None()),
        serde_json::Value::Bool(b) => to_py_obj(py, *b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                to_py_obj(py, i)
            } else if let Some(u) = n.as_u64() {
                to_py_obj(py, u)
            } else if let Some(f) = n.as_f64() {
                to_py_obj(py, f)
            } else {
                Ok(py.None())
            }
        }
        serde_json::Value::String(s) => to_py_obj(py, s.as_str()),
        serde_json::Value::Array(arr) => {
            let list = PyList::empty(py);
            for item in arr {
                list.append(json_value_to_py(py, item)?)?;
            }
            Ok(list.into_any().unbind())
        }
        serde_json::Value::Object(map) => {
            let dict = PyDict::new(py);
            for (k, val) in map {
                dict.set_item(k, json_value_to_py(py, val)?)?;
            }
            Ok(dict.into_any().unbind())
        }
    }
}

fn health_summary_to_py(py: Python<'_>, s: HealthSummary) -> PyResult<Py<PyAny>> {
    // HealthSummary and nested types derive Serialize in core.
    let v = serde_json::to_value(&s).map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
    json_value_to_py(py, &v)
}

fn device_statuses_to_py(py: Python<'_>, v: Vec<DeviceStatus>) -> PyResult<Py<PyAny>> {
    let j = serde_json::to_value(&v).map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
    json_value_to_py(py, &j)
}

fn folder_statuses_to_py(py: Python<'_>, v: Vec<FolderStatus>) -> PyResult<Py<PyAny>> {
    let j = serde_json::to_value(&v).map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
    json_value_to_py(py, &j)
}

fn diagnostics_to_py(py: Python<'_>, v: Vec<api::DiagnosticCheck>) -> PyResult<Py<PyAny>> {
    let j = serde_json::to_value(&v).map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
    json_value_to_py(py, &j)
}

#[pyclass]
struct Client {
    state: Option<ApiState>,
}

#[pymethods]
impl Client {
    #[new]
    #[pyo3(signature = (data_dir=None))]
    fn new(py: Python<'_>, data_dir: Option<String>) -> PyResult<Self> {
        let data_dir = match data_dir {
            Some(d) if !d.is_empty() => d,
            _ => default_data_dir()?,
        };
        let state = anyhow_block_on(py, async move { api::init_engine(data_dir).await })?;
        Ok(Self { state: Some(state) })
    }

    fn __enter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    #[pyo3(signature = (_exc_type=None, _exc=None, _tb=None))]
    fn __exit__(
        &mut self,
        py: Python<'_>,
        _exc_type: Option<PyObject>,
        _exc: Option<PyObject>,
        _tb: Option<PyObject>,
    ) -> PyResult<bool> {
        self.close(py)?;
        Ok(false)
    }

    fn close(&mut self, py: Python<'_>) -> PyResult<()> {
        if let Some(state) = self.state.take() {
            // Best-effort stop of servers started via this state. Note: core
            // stop_server currently stops all globally-registered servers.
            let _ = anyhow_block_on(py, async move { api::stop_server(&state).await });
        }
        Ok(())
    }

    fn is_closed(&self) -> bool {
        self.state.is_none()
    }

    fn data_dir(&self) -> PyResult<String> {
        Ok(self.state_ref()?.data_dir.clone())
    }

    fn device_id(&self) -> PyResult<String> {
        Ok(api::device_id(self.state_ref()?))
    }

    fn device_name(&self) -> PyResult<String> {
        Ok(api::device_name(self.state_ref()?))
    }

    #[pyo3(signature = (name))]
    fn set_device_name(&self, py: Python<'_>, name: String) -> PyResult<String> {
        let state = self.state_ref()?.clone();
        anyhow_block_on(py, async move { api::set_device_name(&state, name).await })
    }

    #[pyo3(signature = (timeout=5))]
    fn discover(&self, py: Python<'_>, timeout: u64) -> PyResult<Vec<Py<PyAny>>> {
        let peers = anyhow_block_on(py, async move { api::discover_devices(timeout).await })?;
        peers.into_iter().map(|d| discovered_to_py(py, d)).collect()
    }

    #[pyo3(signature = (ip, port))]
    fn pair(&self, py: Python<'_>, ip: String, port: u16) -> PyResult<String> {
        let state = self.state_ref()?.clone();
        anyhow_block_on(
            py,
            async move { api::pair_with_device(&state, ip, port).await },
        )
    }

    fn devices(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        let list = api::list_devices(self.state_ref()?)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        list.into_iter()
            .map(|d| device_entry_to_py(py, d))
            .collect()
    }

    fn remove_device(&self, py: Python<'_>, device_id: String) -> PyResult<Py<PyAny>> {
        let cleanup = api::remove_device(self.state_ref()?, device_id)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        device_cleanup_to_py(py, cleanup)
    }

    #[pyo3(signature = (local_path, device_id, direction="bidirectional"))]
    fn add_folder(
        &self,
        _py: Python<'_>,
        local_path: String,
        device_id: String,
        direction: &str,
    ) -> PyResult<i64> {
        api::add_sync_folder(
            self.state_ref()?,
            local_path,
            device_id,
            direction.to_string(),
        )
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    fn folders(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        let list = api::list_sync_folders(self.state_ref()?)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        list.into_iter()
            .map(|f| folder_entry_to_py(py, f))
            .collect()
    }

    fn remove_folder(&self, _py: Python<'_>, folder_id: i64) -> PyResult<()> {
        api::remove_folder(self.state_ref()?, folder_id)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    #[pyo3(signature = (folder_id, local_path, port=9847))]
    fn serve(
        &self,
        py: Python<'_>,
        folder_id: i64,
        local_path: String,
        port: u16,
    ) -> PyResult<u16> {
        let state = self.state_ref()?.clone();
        anyhow_block_on(py, async move {
            api::start_server(&state, port, folder_id, local_path).await
        })
    }

    fn stop_servers(&self, py: Python<'_>) -> PyResult<()> {
        let state = self.state_ref()?.clone();
        anyhow_block_on(py, async move { api::stop_server(&state).await })
    }

    fn pending_pairings(&self, _py: Python<'_>) -> PyResult<Vec<(String, String)>> {
        api::pending_pairings(self.state_ref()?).map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    fn approve_pairing(
        &self,
        _py: Python<'_>,
        device_id: String,
        device_name: String,
    ) -> PyResult<()> {
        api::approve_pending_pairing(self.state_ref()?, device_id, device_name)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    fn deny_pairing(&self, _py: Python<'_>, device_id: String) -> PyResult<()> {
        api::deny_pending_pairing(self.state_ref()?, device_id)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (folder_id, local_path, remote_ip, remote_port, device_id, dry_run=false))]
    fn sync(
        &self,
        py: Python<'_>,
        folder_id: i64,
        local_path: String,
        remote_ip: String,
        remote_port: u16,
        device_id: String,
        dry_run: bool,
    ) -> PyResult<Py<PyAny>> {
        let state = self.state_ref()?.clone();
        let result = anyhow_block_on(py, async move {
            api::sync_folder(
                &state,
                folder_id,
                local_path,
                remote_ip,
                remote_port,
                device_id,
                dry_run,
            )
            .await
        })?;
        sync_result_to_py(py, result)
    }

    fn poll_events(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        let state = self.state_ref()?.clone();
        let events = anyhow_block_on(py, async move {
            Ok::<_, anyhow::Error>(api::poll_sync_events(&state).await)
        })?;
        events
            .into_iter()
            .map(|e| sync_event_to_py(py, e))
            .collect()
    }

    #[pyo3(signature = (limit=20))]
    fn sessions(&self, py: Python<'_>, limit: u32) -> PyResult<Vec<Py<PyAny>>> {
        let list = api::list_recent_sessions(self.state_ref()?, limit)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        list.into_iter()
            .map(|s| session_entry_to_py(py, s))
            .collect()
    }

    fn conflicts(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        let list = api::list_conflicts(self.state_ref()?)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        list.into_iter()
            .map(|c| conflict_entry_to_py(py, c))
            .collect()
    }

    #[pyo3(signature = (folder_id, backup_path, action))]
    fn resolve_conflict(
        &self,
        py: Python<'_>,
        folder_id: i64,
        backup_path: String,
        action: String,
    ) -> PyResult<String> {
        let state = self.state_ref()?.clone();
        anyhow_block_on(py, async move {
            api::resolve_conflict(&state, folder_id, backup_path, action).await
        })
    }

    #[pyo3(signature = (folder_id, winner_path, loser_path))]
    fn read_conflict(
        &self,
        py: Python<'_>,
        folder_id: i64,
        winner_path: String,
        loser_path: String,
    ) -> PyResult<Py<PyAny>> {
        let c = api::read_conflict_contents(self.state_ref()?, folder_id, winner_path, loser_path)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        conflict_contents_to_py(py, c)
    }

    fn overall_health(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let s = api::overall_health(self.state_ref()?)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        health_summary_to_py(py, s)
    }

    fn device_statuses(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let v = api::device_statuses(self.state_ref()?)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        device_statuses_to_py(py, v)
    }

    fn folder_statuses(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let v = api::folder_statuses(self.state_ref()?)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        folder_statuses_to_py(py, v)
    }

    fn diagnostics(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let state = self.state_ref()?.clone();
        let v = anyhow_block_on(py, async move {
            Ok::<_, anyhow::Error>(api::run_diagnostics(&state).await)
        })?;
        diagnostics_to_py(py, v)
    }

    fn shared_folders(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        let list = api::list_my_shared_folders(self.state_ref()?)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        list.into_iter()
            .map(|s| shared_folder_to_py(py, s))
            .collect()
    }

    fn pending_folder_pairings(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        let list = api::pending_folder_pairings(self.state_ref()?)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        list.into_iter()
            .map(|p| pending_folder_pairing_to_py(py, p))
            .collect()
    }

    #[pyo3(signature = (folder_id=None, limit=50))]
    fn file_history(
        &self,
        py: Python<'_>,
        folder_id: Option<i64>,
        limit: u32,
    ) -> PyResult<Vec<Py<PyAny>>> {
        let list = api::list_file_history(self.state_ref()?, folder_id, limit)
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        list.into_iter()
            .map(|h| file_history_to_py(py, h))
            .collect()
    }
}

impl Client {
    fn state_ref(&self) -> PyResult<&ApiState> {
        self.state
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("client is closed"))
    }
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Client>()?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
