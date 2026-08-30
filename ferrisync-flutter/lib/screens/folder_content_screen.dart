import 'dart:io';

import 'package:flutter/material.dart';
import 'package:intl/intl.dart';
import '../models/sync_models.dart';
import '../theme/ferri_theme.dart';

/// Read-only browser for a sync folder's contents: breadcrumbs, search, sort,
/// list/grid toggle, pull-to-refresh, type-aware icons, date grouping and
/// per-file sync badges (`✓` synced / `↑` pushed / `↓` pulled / `!` conflict).
///
/// [load], [states] and [_now] are injectable so widget tests can avoid real
/// filesystem I/O (which never settles under the test FakeAsync zone).
class FolderContentScreen extends StatefulWidget {
  const FolderContentScreen({
    super.key,
    required this.folder,
    this.load,
    this.states,
  });

  final SyncFolder folder;
  final Future<List<FileSystemEntity>> Function(String path)? load;

  /// Mapping of folder-relative path → last recorded sync action.
  final Map<String, String>? states;

  @override
  State<FolderContentScreen> createState() => _FolderContentScreenState();
}

enum _SortField { name, size, modified }

class _FolderContentScreenState extends State<FolderContentScreen> {
  late String _currentPath;
  late Future<List<FileSystemEntity>> _listing;
  String _query = '';
  _SortField _sortField = _SortField.name;
  bool _sortAsc = true;
  bool _grid = false;
  late final Map<String, String> _states = widget.states ?? const {};

  static final DateFormat _dateFormat = DateFormat('yyyy-MM-dd HH:mm');

  Future<List<FileSystemEntity>> _defaultLoad(String path) async {
    final dir = Directory(path);
    if (!dir.existsSync()) {
      throw FileSystemException('Folder does not exist', path);
    }
    return dir.list().toList();
  }

  DateTime? _now() => DateTime.now();

  String get _root => widget.folder.localPath;

  @override
  void initState() {
    super.initState();
    _currentPath = _root;
    _listing = _start(_currentPath);
  }

  Future<List<FileSystemEntity>> _start(String path) =>
      (widget.load ?? _defaultLoad)(path);

  String _nameOf(FileSystemEntity e) =>
      e.path.split(Platform.pathSeparator).last;

  /// Path of [e] relative to the folder root (what history entries use).
  String _relOf(FileSystemEntity e) {
    var rel = e.path;
    var base = _root;
    if (rel.startsWith(base)) {
      rel = rel.substring(base.length);
    }
    while (rel.startsWith('/') || rel.startsWith(r'\')) {
      rel = rel.substring(1);
    }
    return rel;
  }

  List<FileSystemEntity> _sort(Iterable<FileSystemEntity> items) {
    final list = items.toList();
    list.sort((a, b) {
      final aDir = a is Directory;
      final bDir = b is Directory;
      if (aDir != bDir) return aDir ? -1 : 1;
      final aFile = a is File ? a : null;
      final bFile = b is File ? b : null;
      int cmp;
      // ignore: prefer_switch_expression
      switch (_sortField) {
        case _SortField.name:
          cmp = _nameOf(a).toLowerCase().compareTo(_nameOf(b).toLowerCase());
        case _SortField.size:
          cmp = (aFile?.lengthSync() ?? -1).compareTo(bFile?.lengthSync() ?? -1);
        case _SortField.modified:
          cmp = _modifiedOf(a).compareTo(_modifiedOf(b));
      }
      return _sortAsc ? cmp : -cmp;
    });
    return list;
  }

  DateTime _modifiedOf(FileSystemEntity e) {
    try {
      if (e is File) return e.statSync().modified;
      if (e is Directory) return e.statSync().modified;
    } catch (_) {}
    return DateTime.fromMillisecondsSinceEpoch(0);
  }

  List<FileSystemEntity> _filter(List<FileSystemEntity> items) {
    final q = _query.trim().toLowerCase();
    if (q.isEmpty) return items;
    return items.where((e) => _nameOf(e).toLowerCase().contains(q)).toList();
  }

  void _reload() {
    setState(() => _listing = _start(_currentPath));
  }

  void _enter(String path) {
    setState(() {
      _currentPath = path;
      _listing = _start(_currentPath);
    });
  }

  Future<void> _refresh() async {
    setState(() => _listing = _start(_currentPath));
    await _listing.catchError((_) => <FileSystemEntity>[]);
  }

  @override
  Widget build(BuildContext context) {
    final atRoot = _currentPath == _root;
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
                _enter(Directory(_currentPath).parent.path);
              }),
        actions: [
          IconButton(
            tooltip: _grid ? 'List view' : 'Grid view',
            icon: Icon(_grid ? Icons.view_list : Icons.grid_view),
            onPressed: () => setState(() => _grid = !_grid),
          ),
          PopupMenuButton<_SortField>(
            tooltip: 'Sort',
            icon: const Icon(Icons.sort),
            onSelected: (field) => setState(() {
              if (_sortField == field) {
                _sortAsc = !_sortAsc;
              } else {
                _sortField = field;
                _sortAsc = true;
              }
            }),
            itemBuilder: (_) => [
              CheckedPopupMenuItem(
                value: _SortField.name,
                checked: _sortField == _SortField.name,
                child: const Text('Name'),
              ),
              CheckedPopupMenuItem(
                value: _SortField.size,
                checked: _sortField == _SortField.size,
                child: const Text('Size'),
              ),
              CheckedPopupMenuItem(
                value: _SortField.modified,
                checked: _sortField == _SortField.modified,
                child: const Text('Modified'),
              ),
            ],
          ),
        ],
        bottom: PreferredSize(
          preferredSize: const Size.fromHeight(56),
          child: ColoredBox(
            color: Theme.of(context).colorScheme.surface,
            child: Padding(
              padding: const EdgeInsets.fromLTRB(16, 4, 16, 8),
              child: TextField(
                key: const ValueKey('folder_search'),
                onChanged: (v) => setState(() => _query = v),
                textInputAction: TextInputAction.search,
                decoration: InputDecoration(
                  hintText: 'Search files',
                  prefixIcon: const Icon(Icons.search),
                  isDense: true,
                  counterText: '',
                  border: OutlineInputBorder(
                    borderRadius: BorderRadius.circular(FerriTokens.radiusM),
                  ),
                  suffixIcon: _query.isEmpty
                      ? null
                      : IconButton(
                          icon: const Icon(Icons.clear),
                          onPressed: () => setState(() => _query = ''),
                        ),
                ),
              ),
            ),
          ),
        ),
      ),
      body: Column(
        children: [
          _Breadcrumbs(
            rootLabel: _root.split(Platform.pathSeparator).last,
            currentPath: _currentPath,
            rootPath: _root,
            onSelect: (p) {
              if (p != _currentPath) _enter(p);
            },
          ),
          Expanded(
            child: RefreshIndicator(
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
                  final items = _filter(_sort(snapshot.data ?? const []));
                  if (items.isEmpty) {
                    return _emptyView(_query.trim().isNotEmpty);
                  }
                  if (_grid) return _gridView(items);
                  return _listView(items);
                },
              ),
            ),
          ),
        ],
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

  Widget _emptyView(bool searching) {
    return ListView(
      physics: const AlwaysScrollableScrollPhysics(),
      children: [
        const SizedBox(height: 96),
        Icon(
          searching ? Icons.search_off : Icons.folder_open,
          size: 48,
          color: Theme.of(context).colorScheme.outline,
        ),
        const SizedBox(height: 12),
        Center(
          child: Text(
            searching
                ? 'No matches for "${_query.trim()}"'
                : 'Nothing here yet — sync to bring files over',
          ),
        ),
      ],
    );
  }

  Widget _listView(List<FileSystemEntity> items) {
    // In "modified" sort mode we spl+ date headers between buckets; otherwise a
    // flat deterministic list (dirs first) keeps tests and navigation simple.
    if (_sortField != _SortField.modified) {
      return ListView(
        physics: const AlwaysScrollableScrollPhysics(),
        keyboardDismissBehavior: ScrollViewKeyboardDismissBehavior.onDrag,
        children: [for (final e in items) _entryTile(e)],
      );
    }

    final seen = <String>{};
    final children = <Widget>[];
    for (final e in items) {
      final label = _bucketLabel(_modifiedOf(e));
      if (seen.add(label)) children.add(_dateHeader(label));
      children.add(_entryTile(e));
    }
    return ListView(
      physics: const AlwaysScrollableScrollPhysics(),
      keyboardDismissBehavior: ScrollViewKeyboardDismissBehavior.onDrag,
      children: children,
    );
  }

  String _bucketLabel(DateTime modified) {
    final today = _now()!;
    final date = DateTime(modified.year, modified.month, modified.day);
    final todayDate = DateTime(today.year, today.month, today.day);
    final diff = todayDate.difference(date).inDays;
    if (diff <= 0) return 'Today';
    if (diff == 1) return 'Yesterday';
    return DateFormat('MMMM d, yyyy').format(date);
  }

  Widget _dateHeader(String label) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 12, 16, 4),
      child: Text(
        label.toUpperCase(),
        style: Theme.of(context).textTheme.labelSmall!.copyWith(
              letterSpacing: 1.1,
              fontWeight: FontWeight.w700,
              color: Theme.of(context).colorScheme.outline,
            ),
      ),
    );
  }

  Widget _gridView(List<FileSystemEntity> items) {
    return GridView.builder(
      physics: const AlwaysScrollableScrollPhysics(),
      padding: const EdgeInsets.all(12),
      gridDelegate: const SliverGridDelegateWithMaxCrossAxisExtent(
        maxCrossAxisExtent: 160,
        mainAxisSpacing: 8,
        crossAxisSpacing: 8,
      ),
      itemCount: items.length,
      itemBuilder: (_, i) => _gridTile(items[i]),
    );
  }

  Widget _gridTile(FileSystemEntity entity) {
    final isDir = entity is Directory;
    return InkWell(
      key: ValueKey(entity.path),
      borderRadius: BorderRadius.circular(FerriTokens.radiusM),
      onTap: isDir ? () => _enter(entity.path) : null,
      child: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          Icon(
            isDir ? Icons.folder_outlined : _iconForFile(entity.path),
            size: 40,
            color: isDir
                ? Theme.of(context).colorScheme.primary
                : Theme.of(context).colorScheme.onSurfaceVariant,
          ),
          const SizedBox(height: 8),
          Text(
            _nameOf(entity),
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            textAlign: TextAlign.center,
          ),
          if (!isDir)
            Padding(
              padding: const EdgeInsets.only(top: 2),
              child: Text(
                _fileMeta(entity as File),
                style: Theme.of(context)
                    .textTheme
                    .bodySmall!
                    .copyWith(color: Theme.of(context).colorScheme.outline),
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
              ),
            ),
        ],
      ),
    );
  }

  Widget _entryTile(FileSystemEntity entity) {
    final isDir = entity is Directory;
    final state = _states[_relOf(entity)];
    final badge = isDir ? null : _syncBadge(state);

    return ListTile(
      key: ValueKey(entity.path),
      leading: Icon(
        isDir ? Icons.folder_outlined : _iconForFile(entity.path),
        color: isDir
            ? Theme.of(context).colorScheme.primary
            : Theme.of(context).colorScheme.onSurfaceVariant,
      ),
      title: Text(_nameOf(entity), overflow: TextOverflow.ellipsis),
      subtitle: isDir ? null : Text(_fileMeta(entity as File)),
      onTap: isDir ? () => _enter(entity.path) : null,
      trailing: badge,
    );
  }

  /// ✓ synced · ↑ pushed · ↓ pulled · ! conflict (from recorded history).
  Widget? _syncBadge(String? action) {
    if (action == null) return null;
    final a = action.toLowerCase();
    final (icon, color) = a.contains('conflict')
        ? (Icons.priority_high, Theme.of(context).colorScheme.error)
        : a.contains('pull')
            ? (Icons.arrow_downward, Theme.of(context).colorScheme.tertiary)
            : a.contains('push')
                ? (Icons.arrow_upward, Theme.of(context).colorScheme.primary)
                : (Icons.check_circle, Theme.of(context).colorScheme.outline);
    return Icon(icon, size: 16, color: color);
  }

  IconData _iconForFile(String path) {
    final ext = path.split('.').last.toLowerCase();
    switch (ext) {
      case 'jpg' || 'jpeg' || 'png' || 'gif' || 'webp' || 'heic' || 'svg':
        return Icons.image_outlined;
      case 'mp4' || 'mkv' || 'mov' || 'avi' || 'webm':
        return Icons.movie_outlined;
      case 'mp3' || 'wav' || 'flac' || 'ogg' || 'm4a':
        return Icons.music_note_outlined;
      case 'pdf':
        return Icons.picture_as_pdf_outlined;
      case 'zip' || 'tar' || 'gz' || '7z' || 'rar':
        return Icons.folder_zip_outlined;
      case 'txt' || 'md':
        return Icons.description_outlined;
      case 'dart' || 'rs' || 'py' || 'js' || 'ts' || 'go' || 'c' || 'cpp' || 'java':
        return Icons.code_outlined;
      case 'doc' || 'docx' || 'odt':
        return Icons.article_outlined;
      case 'xls' || 'xlsx' || 'csv' || 'ods':
        return Icons.table_chart_outlined;
      case 'ppt' || 'pptx':
        return Icons.slideshow_outlined;
      default:
        return Icons.insert_drive_file_outlined;
    }
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

/// Clickable path segments from the folder root down to the current directory.
class _Breadcrumbs extends StatelessWidget {
  const _Breadcrumbs({
    required this.rootLabel,
    required this.currentPath,
    required this.rootPath,
    required this.onSelect,
  });

  final String rootLabel;
  final String currentPath;
  final String rootPath;
  final void Function(String path) onSelect;

  @override
  Widget build(BuildContext context) {
    if (currentPath == rootPath) return const SizedBox(height: 8);

    var rest = currentPath.substring(rootPath.length);
    final segments = <(String, String)>[]; // (label, full path)
    var cursor = rootPath;
    for (final part in rest.split(Platform.pathSeparator)) {
      if (part.isEmpty) continue;
      cursor = '$cursor${Platform.pathSeparator}$part';
      segments.add((part, cursor));
    }

    return Material(
      color: Theme.of(context).colorScheme.surfaceContainerHighest,
      child: SizedBox(
        height: 40,
        child: SingleChildScrollView(
          scrollDirection: Axis.horizontal,
          padding: const EdgeInsets.symmetric(horizontal: 12),
          child: Row(
            children: [
              _crumb(context, rootLabel, rootPath, isRoot: true),
              for (final (i, (label, path)) in segments.indexed) ...[
                const Padding(
                  padding: EdgeInsets.symmetric(horizontal: 2),
                  child: Icon(Icons.chevron_right, size: 16),
                ),
                _crumb(context, label, path),
                if (i == segments.length - 1) const SizedBox(width: 12),
              ],
            ],
          ),
        ),
      ),
    );
  }

  Widget _crumb(BuildContext context, String label, String full, {bool isRoot = false}) {
    final isCurrent = full == currentPath;
    final style = Theme.of(context).textTheme.bodySmall!.copyWith(
          fontWeight: isCurrent ? FontWeight.w700 : FontWeight.w500,
          color: isCurrent
              ? Theme.of(context).colorScheme.primary
              : Theme.of(context).colorScheme.onSurfaceVariant,
        );
    return Padding(
      padding: EdgeInsets.only(left: isRoot ? 4 : 0),
      child: InkWell(
        onTap: isCurrent ? () {} : () => onSelect(full),
        borderRadius: BorderRadius.circular(4),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 8),
          child: Text(label, style: style),
        ),
      ),
    );
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