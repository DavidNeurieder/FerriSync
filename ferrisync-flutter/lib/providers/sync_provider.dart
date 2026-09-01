import 'dart:async';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:path_provider/path_provider.dart';
import '../gen/api.dart' as frb;
import '../gen/diagnostics.dart' as frb_diag;
import '../gen/frb_generated.dart';
import '../gen/health.dart' as frb_health;
import '../gen/sync_engine/conflicts.dart' as frb_conflicts;
import '../gen/sync_engine/session.dart' as frb_session;
import '../models/sync_models.dart';
import '../services/android_foreground.dart';
import '../services/notification_service.dart';
import '../utils/format_bytes.dart';

class SyncService extends ChangeNotifier {
  SyncService({NotificationsApi? notifications})
      : _notifications = notifications ?? NotificationsService();

  final NotificationsApi _notifications;
  frb.ApiState? _state;
  String _deviceId = '';
  String _deviceName = '';
  List<Device> _devices = [];
  List<SyncFolder> _folders = [];
  frb_health.HealthSummary? _healthSummary;
  List<frb.SessionEntry> _sessions = [];
  List<frb.FileHistoryEntry> _history = [];
  SyncStatus _status = SyncStatus.idle;
  frb_session.SyncResult? _lastResult;
  /// Human-readable message for the most recent sync failure, if any.
  String? _lastErrorMessage;
  /// Folder name (basename) of the folder currently being synced, if any.
  String? _syncingFolderLabel;
  /// File count completed during the current sync (from live engine events).
  int _syncedFilesNow = 0;
  /// Conflict backups discovered across sync folders. Unlike the transient,
  /// polled events these survive app restarts, so they are the source of
  /// truth for the conflicts screens and the attention list.
  List<frb_conflicts.ConflictEntry> _conflicts = [];
  /// Live transfer progress for the running session (from SyncEvent.progress).
  /// Totals come from the reconciled transfer plan, so ratios are honest.
  int _progressFilesDone = 0;
  int _progressFilesTotal = 0;
  int _progressBytesDone = 0;
  int _progressBytesTotal = 0;
  /// Current transfer phase ("starting", "uploading" or "downloading").
  String _progressStage = 'starting';
  /// First snapshot of the active session, used to derive transfer rate/ETA.
  DateTime? _progressStartedAt;
  int _progressStartBytes = 0;
  /// Onboarding marker: true once the user has moved past the welcome screen.
  /// Defaults to true so nothing ever forces the welcome path.
  bool _onboardingSeen = true;
  /// Engine data directory (used for the onboarding marker file).
  String? _dataDir;
  // False until init() actually runs: a service nobody asked to initialize
  // must not render a perpetual "starting" spinner (and wedge pumpAndSettle).
  bool _initializing = false;
  String? _initError;
  bool _notificationsEnabled = false;
  List<(String, String)> _pendingPairings = [];
  /// Attention signals we have already notified the user about this session
  /// (stable keys), so a repeating offline/conflict state doesn't spam.
  final Set<String> _notifiedAttention = {};

  /// How long to wait for the Rust engine before giving up and surfacing an
  /// in-app error instead of hanging on the splash screen forever.
  static const Duration _initTimeout = Duration(seconds: 20);

  String get deviceId => _deviceId;
  String get deviceName => _deviceName;
  List<Device> get devices => _devices;
  List<SyncFolder> get folders => _folders;

  /// Shared overall-health roll-up from the core (devices/folders/conflicts).
  frb_health.HealthSummary? get healthSummary => _healthSummary;

  /// Number of devices currently considered connected, from shared health.
  int get connectedDevices => _healthSummary?.deviceConnected.toInt() ??
      devices.where((d) => d.presence == Presence.connected).length;

  /// Most recent finished sync sessions, newest first (for the activity feed).
  List<frb.SessionEntry> get sessions => _sessions;
  SyncStatus get status => _status;
  bool get initializing => _initializing;

  /// Human-readable message for the most recent sync failure, if any.
  String? get lastErrorMessage => _lastErrorMessage;

  /// Folder name of the folder currently being synced (null when idle).
  String? get syncingFolderLabel => _syncingFolderLabel;

  /// Number of files that completed during the current sync.
  int get syncedFilesNow => _syncedFilesNow;

  /// Conflict backups discovered across configured sync folders.
  List<frb_conflicts.ConflictEntry> get conflicts => _conflicts;

  /// Number of unresolved conflicts. Prefers the persistent folder scan, and
  /// falls back to the history feed before one has run (e.g. in tests).
  int get conflictCount =>
      _conflicts.isNotEmpty ? _conflicts.length : recentConflicts;

  /// Live transfer progress: files completed / total in the current session.
  int get syncFilesDone => _progressFilesDone;
  int get syncFilesTotal => _progressFilesTotal;
  int get syncBytesDone => _progressBytesDone;
  int get syncBytesTotal => _progressBytesTotal;

  /// 0..1 completion derived from the session's transfer plan, or null when
  /// the plan is empty (nothing left to transfer).
  double? get syncProgressValue {
    if (_progressFilesTotal <= 0) return null;
    final done = _progressFilesDone.clamp(0, _progressFilesTotal);
    return done / _progressFilesTotal;
  }

  /// Whether the running session has a meaningful progress ratio yet.
  bool get hasLiveProgress => _progressFilesTotal > 0;

  /// Current transfer phase ("starting", "uploading" or "downloading").
  String get syncStage => _progressStage;

  /// Average transfer rate (bytes/s) across the running session, or null when
  /// there is not enough data yet.
  double? get syncRateBytesPerSec {
    final start = _progressStartedAt;
    if (start == null) return null;
    final elapsed = DateTime.now().difference(start).inMilliseconds / 1000;
    if (elapsed <= 0) return null;
    final moved = (_progressBytesDone - _progressStartBytes).clamp(0, 1 << 62);
    if (moved <= 0) return null;
    return moved / elapsed;
  }

  /// Seconds remaining to finish the current transfer, or null when unknown
  /// (no rate yet, or nothing left to do).
  int? get syncEtaSecs {
    final rate = syncRateBytesPerSec;
    final remaining = _progressBytesTotal - _progressBytesDone;
    if (rate == null || rate <= 0 || remaining <= 0) return null;
    return (remaining / rate).ceil().clamp(0, 1 << 30);
  }

  /// Whether the user has passed the first-launch welcome screen.
  bool get hasCompletedOnboarding => _onboardingSeen;

  /// Whether the Rust engine finished starting up successfully.
  bool get isReady => _state != null;

  /// Number of per-file conflict entries in the recent history feed.
  int get recentConflicts =>
      _history.where((e) => e.action.toLowerCase().contains('conflict')).length;

  /// Unified "what needs me?" list. Only surfaced when non-empty so a healthy
  /// app never interrupts the user.
  List<AttentionItem> get attentionItems {
    final items = <AttentionItem>[];
    final conflicts = conflictCount;
    if (conflicts > 0) {
      items.add(AttentionItem(
        kind: AttentionKind.conflictFiles,
        label: conflicts == 1
            ? '1 file has a conflict'
            : '$conflicts files have conflicts',
      ));
    }
    for (final d in _devices.where((d) => !d.isOnline && d.lastSeen > 0)) {
      items.add(AttentionItem(
        kind: AttentionKind.offlineDevice,
        label: '${d.name} is offline',
      ));
    }
    for (final f in _folders.where((f) => f.health.needsAttention)) {
      final label = switch (f.health) {
        FolderHealth.conflict => 'Conflicts in ${f.localPath.split('/').last}',
        FolderHealth.error => 'Sync error in ${f.localPath.split('/').last}',
        _ => '${f.localPath.split('/').last} is offline',
      };
      items.add(AttentionItem(
        kind: AttentionKind.folderHealth,
        label: label,
      ));
    }
    return items;
  }

  /// The dashboard's headline answer to "is everything OK?".
  FerriStatus get ferriStatus {
    if (_status == SyncStatus.syncing) return FerriStatus.syncing;
    if (_status == SyncStatus.error || attentionItems.isNotEmpty) {
      return FerriStatus.attention;
    }
    return FerriStatus.healthy;
  }

  /// Most recent per-file history entries (for the activity feed), across all
  /// folders, newest first.
  List<frb.FileHistoryEntry> get history => _history;

  /// Non-null when engine startup failed or timed out; the app stays usable
  /// (with degraded data) so the message is visible.
  String? get initError => _initError;

  /// Whether sync-completion notifications should be posted. Reflects the
  /// persisted preference AND the OS notification permission.
  bool get notificationsEnabled => _notificationsEnabled;

  /// Pairing requests waiting for user approval: `(device_name, device_id)`.
  List<(String, String)> get pendingPairings => _pendingPairings;

  Future<void> init() async {
    _initializing = true;
    _initError = null;
    notifyListeners();
    final dir = await getApplicationSupportDirectory();
    final dataDir = '${dir.path}/ferrisync';
    _dataDir = dataDir;

    // Notification preference is independent of the engine; load it even if
    // engine startup later fails.
    _notificationsEnabled =
        await _notifications.getPref() && await _notifications.areEnabled();

    try {
      // Callers other than main() (e.g. integration tests) may not have
      // initialized the bridge yet; RustLib.init throws if called twice.
      // ignore: avoid_print
      if (!RustLib.instance.initialized) {
        await RustLib.init();
      }
      // ignore: avoid_print
      final state = await frb
          .initEngine(dataDir: dataDir)
          .timeout(_initTimeout);
      // ignore: avoid_print
      _state = state;
      _deviceId = await frb.deviceId(state: state);
      _deviceName = await frb.deviceName(state: state);
      await _loadOnboardingState();
    } on TimeoutException {
      _deviceId = '00000000-0000-0000-0000-000000000000';
      _deviceName = 'Flutter Device';
      _initError = 'engine did not start within ${_initTimeout.inSeconds}s — '
          'try clearing the app\'s storage and reopening';
    } catch (e) {
      _deviceId = '00000000-0000-0000-0000-000000000000';
      _deviceName = 'Flutter Device';
      _initError = '$e';
    } finally {
      _initializing = false;
      notifyListeners();
    }
  }

  /// Read the first-launch marker. Failures keep the default (already seen)
  /// so a storage hiccup never strands the user on the welcome screen.
  Future<void> _loadOnboardingState() async {
    final dataDir = _dataDir;
    if (dataDir == null) return;
    try {
      _onboardingSeen = await File('$dataDir/onboarding.seen').exists();
    } catch (_) {
      _onboardingSeen = true;
    }
  }

  /// Mark first-launch as done by persisting a marker file next to the engine
  /// data. Safe to call repeatedly.
  Future<void> completeOnboarding() async {
    final dataDir = _dataDir;
    if (dataDir != null) {
      try {
        await File('$dataDir/onboarding.seen').writeAsString('seen');
      } catch (_) {}
    }
    _onboardingSeen = true;
    notifyListeners();
  }

  /// Toggle sync-completion notifications. Enabling runs the Android runtime
  /// permission request first; the preference is only persisted when granted.
  /// Returns whether notifications are enabled afterwards.
  Future<bool> setNotificationsEnabled(bool enabled) async {
    if (!enabled) {
      _notificationsEnabled = false;
      await _notifications.setPref(false);
      notifyListeners();
      return false;
    }
    final granted = await _notifications.requestPermission();
    _notificationsEnabled = granted;
    await _notifications.setPref(granted);
    notifyListeners();
    return granted;
  }

  /// Poll the Rust layer for pairing requests waiting for approval.
  Future<void> pollPendingPairings() async {
    final state = _state;
    if (state == null) return;
    try {
      final pairs = await frb.pendingPairings(state: state);
      _pendingPairings = pairs;
      notifyListeners();
    } catch (_) {}
  }

  /// Approve a pending pairing request. The remote device is written to the
  /// paired-devices table so its next sync attempt is accepted silently.
  Future<String> approvePairing(
      String deviceId, String deviceName) async {
    final state = _state;
    if (state == null) return 'Engine not ready';
    try {
      await frb.approvePendingPairing(
          state: state, deviceId: deviceId, deviceName: deviceName);
      await pollPendingPairings();
      await refresh();
      return 'Paired with $deviceName';
    } catch (e) {
      return 'Approve failed: $e';
    }
  }

  /// Deny a pending pairing request. The device is remembered for this
  /// session so repeated requests are silently rejected.
  Future<String> denyPairing(String deviceId) async {
    final state = _state;
    if (state == null) return 'Engine not ready';
    try {
      await frb.denyPendingPairing(state: state, deviceId: deviceId);
      await pollPendingPairings();
      return 'Pairing denied';
    } catch (e) {
      return 'Deny failed: $e';
    }
  }

  Future<void> refresh() async {
    final state = _state;
    if (state == null) return;

    // Shared semantic state from core: device presence and per-folder health
    // use the exact same thresholds the CLI/REPL do, so the app no longer
    // re-derives "online" from a Dart-side window.
    List<frb_health.DeviceStatus> devStatuses = [];
    List<frb_health.FolderStatus> folderStatuses = [];
    try {
      devStatuses = await frb.deviceStatuses(state: state);
    } catch (_) {}
    try {
      folderStatuses = await frb.folderStatuses(state: state);
    } catch (_) {}
    try {
      _healthSummary = await frb.overallHealth(state: state);
    } catch (_) {}

    final presenceById = {
      for (final d in devStatuses) d.id: _mapPresence(d.presence),
    };
    _devices = (await frb.listDevices(state: state))
        .map((d) => Device(
              id: d.id,
              name: d.name,
              lastSeen: d.lastSeen,
              presence: presenceById[d.id] ?? Presence.offline,
            ))
        .toList();

    final folderHealthById = {
      for (final f in folderStatuses) f.id: _mapFolderHealth(f.health),
    };
    final folderConflictsById = {
      for (final f in folderStatuses) f.id: f.conflicts.toInt(),
    };
    final folderDeviceNameById = {
      for (final f in folderStatuses) f.id: f.deviceName,
    };
    _folders = (await frb.listSyncFolders(state: state))
        .map((f) => SyncFolder(
              id: f.id,
              localPath: f.localPath,
              name: f.name,
              deviceId: f.deviceId,
              direction: f.direction,
              peers: f.peers
                  .map((p) => FolderPeer(
                        deviceId: p.deviceId,
                        mode: p.mode,
                        remotePath: p.remotePath,
                        enabled: p.enabled,
                      ))
                  .toList(),
              lastSyncAt: f.lastSyncAt,
              health: folderHealthById[f.id] ?? FolderHealth.notConfigured,
              deviceName: folderDeviceNameById[f.id],
              conflicts: folderConflictsById[f.id] ?? 0,
            ))
        .toList();

    await refreshSessions();
    await refreshHistory();

    // Conflict inventory is a disk scan across folders; surface it (and any
    // pairing requests) alongside the other state.
    try {
      _conflicts = await frb.listConflicts(state: state);
    } catch (_) {}
    // Surface any pairing requests waiting for approval.
    try {
      _pendingPairings = await frb.pendingPairings(state: state);
    } catch (_) {}

    _maybeNotifyAttention();
    notifyListeners();
  }

  /// Run every on-device diagnostic check (`ferrisync doctor`) through the same
  /// Rust model the CLI uses. Returns an empty list if the engine isn't ready.
  Future<List<frb_diag.DiagnosticCheck>> runDiagnostics() async {
    final state = _state;
    if (state == null) return [];
    try {
      return await frb.runDiagnostics(state: state);
    } catch (_) {
      return [];
    }
  }

  /// Post a notification when something first needs the user's attention this
  /// session (a conflict appeared, or a device went offline). Fires at most
  /// once per distinct signal so polling never floods the tray.
  void _maybeNotifyAttention() {
    final parts = computeAttentionParts(
      conflicts: _conflicts,
      devices: _devices,
      notified: _notifiedAttention,
      enabled: _notificationsEnabled,
    );
    if (parts.isEmpty) return;
    unawaited(_notifications.show(
      title: 'FerriSync',
      body: parts.join(' · '),
    ));
  }

  Presence _mapPresence(frb_health.Presence p) => switch (p) {
        frb_health.Presence.connected => Presence.connected,
        frb_health.Presence.recentlySeen => Presence.recentlySeen,
        frb_health.Presence.offline => Presence.offline,
      };

  FolderHealth _mapFolderHealth(frb_health.FolderHealth h) => switch (h) {
        frb_health.FolderHealth.healthy => FolderHealth.healthy,
        frb_health.FolderHealth.syncing => FolderHealth.syncing,
        frb_health.FolderHealth.waiting => FolderHealth.waiting,
        frb_health.FolderHealth.offline => FolderHealth.offline,
        frb_health.FolderHealth.error => FolderHealth.error,
        frb_health.FolderHealth.conflict => FolderHealth.conflict,
        frb_health.FolderHealth.notConfigured => FolderHealth.notConfigured,
      };

  /// Refresh the recent-session history used by the activity feed.
  Future<void> refreshSessions() async {
    final state = _state;
    if (state == null) return;
    try {
      _sessions = await frb.listRecentSessions(state: state, limit: 60);
    } catch (_) {}
  }

  /// Refresh per-file sync history used by the activity feed.
  Future<void> refreshHistory() async {
    final state = _state;
    if (state == null) return;
    try {
      _history = await frb.listFileHistory(state: state, folderId: null, limit: 60);
    } catch (_) {}
  }

  /// Latest recorded history entry per file for one folder (path → action).
  Future<List<frb.FileHistoryEntry>> historyForFolder(int folderId) async {
    final state = _state;
    if (state == null) return [];
    try {
      return await frb.listFileHistory(state: state, folderId: folderId, limit: 500);
    } catch (_) {
      return [];
    }
  }

  /// Rename this device. Persists in the Rust layer and restarts any running
  /// folder servers so peers see the new name immediately.
  Future<void> setDeviceName(String name) async {
    final state = _state;
    if (state == null) throw StateError('Sync engine not initialized');
    _deviceName = await frb.setDeviceName(state: state, name: name);
    notifyListeners();
  }

  Future<String> pairWithDevice(String ip, int port) async {
    final state = _state;
    if (state == null) {
      await Future.delayed(const Duration(seconds: 1));
      return 'Paired with device at $ip:$port';
    }
    return await frb.pairWithDevice(state: state, ip: ip, port: port);
  }

  /// Rename a paired (remote) device. Returns a user-facing result message.
  Future<String> renameRemoteDevice(String deviceId, String name) async {
    final state = _state;
    if (state == null) return 'Engine not ready';
    try {
      await frb.upsertDevice(state: state, id: deviceId, name: name);
      await refresh();
      return 'Renamed to $name';
    } catch (e) {
      return 'Rename failed: $e';
    }
  }

  Future<List<frb.DiscoveredDevice>> discoverDevices({int timeoutSecs = 3}) async {
    return await frb.discoverDevices(timeoutSecs: BigInt.from(timeoutSecs));
  }

  Future<void> addSyncFolder(String localPath, String deviceId) async {
    final state = _state;
    if (state == null) return;
    final folderId = await frb.addSyncFolder(
      state: state,
      localPath: localPath,
      deviceId: deviceId,
      direction: 'bidirectional',
    );
    // Serve the new folder so the peer can push to us later.
    await startFolderServer(folderId.toInt(), localPath);    await refresh();
  }

  /// Multi-device form: add/extend a folder for several peers at once, each
  /// with its own mode (bidirectional / send_only / receive_only).
  Future<void> addSyncFolderWithPeers(
    String localPath,
    String name,
    List<({String deviceId, String? mode, String? remotePath})> peers,
  ) async {
    final state = _state;
    if (state == null) return;
    final folderId = await frb.addSyncFolderWithPeers(
      state: state,
      localPath: localPath,
      name: name,
      peers: [
        for (final p in peers)
          frb.FolderPeerRequest(
            deviceId: p.deviceId,
            mode: p.mode,
            remotePath: p.remotePath,
          ),
      ],
    );
    await startFolderServer(folderId.toInt(), localPath);
    await refresh();
  }

  /// Dry-run a folder against one of its peers to preview what a real sync
  /// would do, without transferring any files. Returns the plan counts.
  Future<SyncPreview?> previewSyncFolder(SyncFolder folder, String deviceId) async {
    final state = _state;
    if (state == null) return null;
    final matches = folder.peers.where((p) => p.deviceId == deviceId);
    if (matches.isEmpty) return null;
    final addr = await frb.deviceLastAddr(state: state, deviceId: deviceId);
    if (addr == null) return null;
    final parsed = _parseHostPort(addr);
    if (parsed == null) return null;
    try {
      final result = await frb.syncFolder(
        state: state,
        folderId: folder.id,
        localPath: folder.localPath,
        remoteIp: parsed.host,
        remotePort: parsed.port,
        deviceId: deviceId,
        dryRun: true,
      );
      return SyncPreview.fromResult(result);
    } catch (_) {
      return null;
    }
  }

  /// Get a peer's last known address, for showing where a folder points.
  Future<String?> peerAddress(String deviceId) async {
    final state = _state;
    if (state == null) return null;
    return frb.deviceLastAddr(state: state, deviceId: deviceId);
  }

/// Remove every paired device and their folders/history. Returns a
  /// user-facing result message.
  Future<String> removeAllDevices() async {
    final state = _state;
    if (state == null) return 'Engine not ready';
    try {
      final removed = await frb.removeAllDevices(state: state);
      await refresh();
      return removed == BigInt.zero
          ? 'No devices to remove'
          : 'Removed $removed device(s)';
    } catch (e) {
      return 'Failed to remove devices: $e';
    }
  }

  /// Remove a paired device and all its associated data (folders,
  /// metadata, history, sessions). Returns a user-facing result message.
  Future<String> removeDevice(String deviceId) async {
    final state = _state;
    if (state == null) return 'Engine not ready';
    try {
      final c = await frb.removeDevice(state: state, deviceId: deviceId);
      await refresh();
      if (c.deviceRemoved == BigInt.zero) return 'Device not found';
      final parts = <String>[];
      if (c.foldersRemoved > BigInt.zero) {
        parts.add('${c.foldersRemoved} folder(s)');
      }
      if (c.sessionsRemoved > BigInt.zero) {
        parts.add('${c.sessionsRemoved} session(s)');
      }
      if (c.historyRemoved > BigInt.zero) {
        parts.add('${c.historyRemoved} history entry/entries');
      }
      return parts.isEmpty
          ? 'Device removed'
          : 'Device removed — ${parts.join(', ')} deleted';
    } catch (e) {
      return 'Failed to remove device: $e';
    }
  }

  /// Remove a single sync folder and its metadata/history from the local
  /// database. Returns a user-facing result message.
  Future<String> removeFolder(int folderId) async {
    final state = _state;
    if (state == null) return 'Engine not ready';
    try {
      await frb.removeFolder(state: state, folderId: folderId);
      await refresh();
      return 'Folder removed';
    } catch (e) {
      return 'Failed to remove folder: $e';
    }
  }

  /// Resolve a conflict backup. `action` is `keep_backup` (the backup's
  /// version becomes the file), `keep_original` (the real/winner file stays,
  /// backup dropped) or `keep_both` (rename the backup to a plain file).
  /// Returns a plain-language result message.
  Future<String> resolveConflict(
      int folderId, String backupPath, String action) async {
    final state = _state;
    if (state == null) return 'Engine not ready';
    try {
      final loser = await frb.resolveConflict(
        state: state,
        folderId: folderId,
        backupPath: backupPath,
        action: action,
      );
      await refresh();
      final kept = switch (action) {
        'keep_both' => 'both versions kept',
        'keep_backup' => loser == 'local'
            ? 'your local version kept'
            : 'the other version kept',
        'keep_original' => loser == 'local'
            ? 'the other version kept'
            : 'your local version kept',
        _ => 'resolution saved',
      };
      return 'Conflict resolved — $kept';
    } catch (e) {
      return 'Couldn\'t resolve conflict: $e';
    }
  }

  /// Read both conflict versions (winner real file + loser backup) as bounded
  /// UTF-8 text so the compare view can render a diff. Returns null when the
  /// engine isn't ready or a version isn't textual (e.g. binary).
  Future<frb.ConflictContents?> readConflictContents(
      int folderId, String winnerPath, String loserPath) async {
    final state = _state;
    if (state == null) return null;
    try {
      return await frb.readConflictContents(
        state: state,
        folderId: folderId,
        winnerPath: winnerPath,
        loserPath: loserPath,
      );
    } catch (_) {
      return null;
    }
  }

  /// Recorded sessions with a given paired device (typically outgoing ones,
  /// since incoming sessions record the peer by address). Newest first.
  Future<List<frb.SessionEntry>> sessionsForDevice(String deviceId) async {
    final state = _state;
    if (state == null) return [];
    try {
      return await frb.listSessionsForDevice(
          state: state, deviceId: deviceId, limit: 50);
    } catch (_) {
      return [];
    }
  }

  /// Start a listener for a folder, walking up from port 9847 if taken.
  Future<void> startFolderServer(int folderId, String localPath) async {
    final state = _state;
    if (state == null) return;
    for (var port = 9847; port < 9867; port++) {
      try {
        await frb.startServer(
          state: state,
          port: port,
          folderId: folderId,
          localPath: localPath,
        );
        // Keep the process unfrozen while we are reachable for peers.
        await AndroidForeground.startServing();
        return;
      } on Object catch (_) {
        if (port == 9866) rethrow;
      }
    }
  }

  Future<void> syncFolder(
    String path,
    String remoteIp, {
    int remotePort = 9847,
    String? deviceId,
  }) async {
    _status = SyncStatus.syncing;
    _lastResult = null;
    _lastErrorMessage = null;
    _syncingFolderLabel =
        path.split(Platform.pathSeparator).where((s) => s.isNotEmpty).last;
    _syncedFilesNow = 0;
    _progressFilesDone = 0;
    _progressFilesTotal = 0;
    _progressBytesDone = 0;
    _progressBytesTotal = 0;
    _progressStage = 'starting';
    _progressStartedAt = null;
    _progressStartBytes = 0;
    notifyListeners();

    final state = _state;
    if (state == null) {
      await Future.delayed(const Duration(seconds: 2));
      _status = SyncStatus.idle;
      notifyListeners();
      return;
    }

    // Find the folder by path
    final folders = await frb.listSyncFolders(state: state);
    final folder = folders.where((f) => f.localPath == path).firstOrNull;
    if (folder == null) {
      _status = SyncStatus.error;
      notifyListeners();
      return;
    }

    final did = deviceId ?? folder.deviceId;
    _lastResult = await frb.syncFolder(
      state: state,
      folderId: folder.id,
      localPath: path,
      remoteIp: remoteIp,
      remotePort: remotePort,
      deviceId: did,
      dryRun: false,
    );

    // Poll for remaining events
    final events = await frb.pollSyncEvents(state: state);
    for (final event in events) {
      event.when(
        syncing: (_) => _status = SyncStatus.syncing,
        idle: () => _status = SyncStatus.idle,
        error: (message) {
          _status = SyncStatus.error;
          if (message.isNotEmpty) {
            _lastErrorMessage = message;
          }
        },
        pairRequested: (_, __) {
          // A remote device is trying to pair — refresh the pending list
          // so the UI can surface an approval dialog.
          unawaited(pollPendingPairings());
        },
        devicePaired: (_, __) {},
        filePulled: (_, __) {
          _syncedFilesNow++;
        },
        filePushed: (_, __) {
          _syncedFilesNow++;
        },
        conflict: (_, __, ___) {
          _syncedFilesNow++;
        },
        progress: (_, stage, filesDone, filesTotal, bytesDone, bytesTotal) {
          _progressStage = stage.isNotEmpty ? stage : _progressStage;
          _progressFilesDone = filesDone.toInt();
          _progressFilesTotal = filesTotal.toInt();
          _progressBytesDone = bytesDone.toInt();
          _progressBytesTotal = bytesTotal.toInt();
          // First snapshot anchors the elapsed window used for rate/ETA.
          if (_progressStartedAt == null) {
            _progressStartedAt = DateTime.now();
            _progressStartBytes = _progressBytesDone;
          }
          _syncedFilesNow = filesDone.toInt();
          notifyListeners();
        },
      );
    }

    await refresh();
    _status = SyncStatus.idle;
    notifyListeners();
  }

  /// Sync a folder right now against its paired device's last known address.
  /// Returns a user-facing result message.
  Future<String> syncFolderNow(SyncFolder folder) async {
    final state = _state;
    if (state == null) return 'Engine not ready';

    final addr =
        await frb.deviceLastAddr(state: state, deviceId: folder.deviceId);
    if (addr == null) {
      return 'No known address for device — pair again';
    }

    final parsed = _parseHostPort(addr);
    if (parsed == null) {
      return 'Invalid address stored for device: $addr';
    }
    final (:host, :port) = parsed;

    try {
      await syncFolder(folder.localPath, host, remotePort: port);
      final res = _lastResult;
      final message = _status == SyncStatus.error
          ? null
          : res == null
              ? 'Sync complete with $host:$port'
              : 'Sync complete with $host:$port '
                  '(Pushed ${res.pushed.length}, Pulled ${res.pulled.length})';

      // Post-sync notification (best-effort; gated on the user's toggle).
      if (_notificationsEnabled && message != null) {
        final folderName =
            folder.localPath.split(Platform.pathSeparator).where((s) => s.isNotEmpty).last;
        await _notifications.show(
          title: 'FerriSync — $folderName',
          body: message,
        );
      }
      return message ?? 'Sync failed';
    } catch (e) {
      _status = SyncStatus.error;
      _lastErrorMessage = '$e';
      notifyListeners();
      return 'Sync failed: $e';
    }
  }

  static ({String host, int port})? _parseHostPort(String addr) {
    final bracket = RegExp(r'^\[(.+)\]:(\d+)$').firstMatch(addr);
    if (bracket != null) {
      return (host: bracket.group(1)!, port: int.parse(bracket.group(2)!));
    }
    final idx = addr.lastIndexOf(':');
    if (idx <= 0 || idx >= addr.length - 1) return null;
    final port = int.tryParse(addr.substring(idx + 1));
    if (port == null) return null;
    return (host: addr.substring(0, idx), port: port);
  }
}

final syncServiceProvider = ChangeNotifierProvider<SyncService>((ref) {
  return SyncService();
});

final deviceIdProvider = Provider<String>((ref) {
  return ref.watch(syncServiceProvider).deviceId;
});

final deviceNameProvider = Provider<String>((ref) {
  return ref.watch(syncServiceProvider).deviceName;
});

final devicesProvider = Provider<List<Device>>((ref) {
  return ref.watch(syncServiceProvider).devices;
});

final foldersProvider = Provider<List<SyncFolder>>((ref) {
  return ref.watch(syncServiceProvider).folders;
});

final syncStatusProvider = Provider<SyncStatus>((ref) {
  return ref.watch(syncServiceProvider).status;
});

/// User's theme preference. Dark-first identity; light is opt-in from Settings.
final themeModeProvider =
    StateProvider<ThemeMode>((ref) => ThemeMode.dark);

/// Latest per-file sync action for a folder's contents (relative path → action),
/// sourced from recorded file history; powers the ✓/↑/↓/! badges in the browser.
final folderFileStatesProvider =
    FutureProvider.family<Map<String, String>, SyncFolder>(
        (ref, folder) async {
  try {
    final entries = await ref.read(syncServiceProvider).historyForFolder(folder.id);
    final byPath = <String, String>{};
    for (final e in entries) {
      byPath[e.path] = e.action;
    }
    return byPath;
  } catch (_) {
    return {};
  }
});

/// Summed size of every file under a sync folder's local path.
/// Falls back to '—' when the path is missing/unreadable (e.g. tests).
final folderSizeProvider = FutureProvider.family<String, int>((ref, folderId) async {
  final folders = ref.watch(foldersProvider);
  final folder = folders.where((f) => f.id == folderId).firstOrNull;
  if (folder == null) return '—';
  final dir = Directory(folder.localPath);
  try {
    var total = 0;
    await for (final entity
        in dir.list(recursive: true, followLinks: false)) {
      if (total > 1 << 40) break; // cap at 1 TiB of bookkeeping
      if (entity is File) total += await entity.length();
    }
    return formatBytes(total);
  } catch (_) {
    return '—';
  }
});

/// Pure helper backing [SyncService]'s attention notifications. Given the
/// current conflict inventory and device presence, it computes which signals
/// are NEW (not already in [notified]) and returns the human-readable
/// notification lines, mutating [notified] in place so each distinct signal
/// is announced at most once per session. Returns an empty list when nothing
/// is new (or notifications are disabled).
List<String> computeAttentionParts({
  required List<frb_conflicts.ConflictEntry> conflicts,
  required List<Device> devices,
  required Set<String> notified,
  required bool enabled,
}) {
  if (!enabled) return const [];
  final parts = <String>[];
  var conflictNew = 0;
  var offlineNew = 0;

  for (final c in conflicts) {
    final key = 'conflict:${c.folderId}:${c.backupPath}';
    if (notified.add(key)) conflictNew++;
  }
  for (final d in devices) {
    if (d.presence == Presence.offline && d.lastSeen > 0) {
      if (notified.add('offline:${d.id}')) offlineNew++;
    }
  }

  if (conflictNew > 0) {
    parts.add(conflictNew == 1
        ? '1 conflict needs your attention'
        : '$conflictNew conflicts need your attention');
  }
  if (offlineNew > 0) {
    parts.add(offlineNew == 1
        ? '1 device is offline'
        : '$offlineNew devices are offline');
  }
  return parts;
}
