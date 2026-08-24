import 'dart:io';

import 'package:flutter/material.dart';
import 'package:intl/intl.dart';
import '../models/sync_models.dart';

/// Read-only browser for a sync folder's contents. Supports drilling into
/// subdirectories; files are listed but not opened (v1).
///
/// [load] is injectable so widget tests can avoid real filesystem I/O
/// (which never settles under the test FakeAsync zone).
class FolderContentScreen extends StatefulWidget {
  const FolderContentScreen({
    super.key,
    required this.folder,
    this.load,
  });

  final SyncFolder folder;
  final Future<List<FileSystemEntity>> Function(String path)? load;

  @override
  State<FolderContentScreen> createState() => _FolderContentScreenState();
}

class _FolderContentScreenState extends State<FolderContentScreen> {
  late String _currentPath;
  late Future<List<FileSystemEntity>> _listing;

  static final DateFormat _dateFormat = DateFormat('yyyy-MM-dd HH:mm');

  Future<List<FileSystemEntity>> _defaultLoad(String path) async {
    final dir = Directory(path);
    if (!dir.existsSync()) {
      throw FileSystemException('Folder does not exist', path);
    }
    return dir.list().toList();
  }

  @override
  void initState() {
    super.initState();
    _currentPath = widget.folder.localPath;
    _listing = _start(_currentPath);
  }

  Future<List<FileSystemEntity>> _start(String path) =>
      (widget.load ?? _defaultLoad)(path);

  List<FileSystemEntity> _sort(Iterable<FileSystemEntity> items) {
    final list = items.toList();
    list.sort((a, b) {
      final aDir = a is Directory;
      final bDir = b is Directory;
      if (aDir != bDir) return aDir ? -1 : 1;
      return _nameOf(a).toLowerCase().compareTo(_nameOf(b).toLowerCase());
    });
    return list;
  }

  String _nameOf(FileSystemEntity e) =>
      e.path.split(Platform.pathSeparator).last;

  void _reload() {
    setState(() => _listing = _start(_currentPath));
  }

  void _enter(FileSystemEntity entity) {
    setState(() {
      _currentPath = entity.path;
      _listing = _start(_currentPath);
    });
  }

  Future<void> _refresh() async {
    setState(() => _listing = _start(_currentPath));
    await _listing.catchError((_) => <FileSystemEntity>[]);
  }

  @override
  Widget build(BuildContext context) {
    final atRoot = _currentPath == widget.folder.localPath;
    return Scaffold(
      appBar: AppBar(
        title: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              _nameOf(Directory(_currentPath)),
              style: Theme.of(context).textTheme.titleMedium,
            ),
            Text(
              _currentPath,
              style: Theme.of(context).textTheme.bodySmall,
              overflow: TextOverflow.ellipsis,
            ),
          ],
        ),
        leading: atRoot
            ? null
            : BackButton(onPressed: () {
                _enter(Directory(_currentPath).parent);
              }),
      ),
      body: RefreshIndicator(
        onRefresh: _refresh,
        child: FutureBuilder<List<FileSystemEntity>>(
          future: _listing,
          builder: (context, snapshot) {
            if (snapshot.connectionState == ConnectionState.waiting) {
              return const Center(child: CircularProgressIndicator());
            }
            if (snapshot.hasError) {
              return _errorView(snapshot.error!);
            }
            final items = _sort(snapshot.data ?? const <FileSystemEntity>[]);
            if (items.isEmpty) {
              return ListView(
                children: const [
                  SizedBox(height: 96),
                  Icon(Icons.folder_open, size: 48),
                  SizedBox(height: 12),
                  Center(
                    child: Text('Nothing here yet — sync to bring files over'),
                  ),
                ],
              );
            }
            return ListView.builder(
              itemCount: items.length,
              itemBuilder: (_, i) => _entryTile(items[i]),
            );
          },
        ),
      ),
    );
  }

  Widget _errorView(Object error) {
    return ListView(
      children: [
        const SizedBox(height: 96),
        const Icon(Icons.error_outline, size: 48),
        const SizedBox(height: 12),
        Center(child: Text('Cannot read folder:\n$_currentPath')),
        const SizedBox(height: 16),
        Center(
          child: FilledButton.tonal(
            onPressed: _reload,
            child: const Text('Retry'),
          ),
        ),
      ],
    );
  }

  Widget _entryTile(FileSystemEntity entity) {
    final isDir = entity is Directory;
    return ListTile(
      key: ValueKey(entity.path),
      leading: Icon(isDir ? Icons.folder : Icons.insert_drive_file),
      title: Text(_nameOf(entity)),
      subtitle: isDir ? null : Text(_fileMeta(entity as File)),
      onTap: isDir ? () => _enter(entity) : null,
    );
  }

  String _fileMeta(File file) {
    try {
      final stat = file.statSync();
      return '${_formatBytes(stat.size)} · ${_dateFormat.format(stat.modified)}';
    } catch (_) {
      return '';
    }
  }
}

String _formatBytes(int bytes) {
  if (bytes < 1024) return '$bytes B';
  if (bytes < 1024 * 1024) return '${(bytes / 1024).toStringAsFixed(1)} KB';
  if (bytes < 1024 * 1024 * 1024) {
    return '${(bytes / (1024 * 1024)).toStringAsFixed(1)} MB';
  }
  return '${(bytes / (1024 * 1024 * 1024)).toStringAsFixed(1)} GB';
}
