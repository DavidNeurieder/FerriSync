import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:ferrisync/models/sync_models.dart';
import 'package:ferrisync/screens/folder_content_screen.dart';

SyncFolder _folder(String path) => SyncFolder(
      id: 1,
      localPath: path,
      deviceId: 'device-1',
      direction: 'bidirectional',
      lastSyncAt: 0,
    );

/// Real filesystem entities are prepared synchronously at test setup; the
/// injected loader just hands them back so no I/O runs under FakeAsync.
Future<List<FileSystemEntity>> Function(String) _loader(
    String forPath, List<FileSystemEntity> entities) {
  return (path) async {
    if (path != forPath) {
      throw FileSystemException('Folder does not exist', path);
    }
    return entities;
  };
}

Widget _wrap(Widget child) => MaterialApp(home: Scaffold(body: child));

void main() {
  testWidgets('lists files and directories sorted, dirs first',
      (tester) async {
    final dir = Directory.systemTemp.createTempSync('ferri_browse');
    addTearDown(() => dir.deleteSync(recursive: true));
    File('${dir.path}/zeta.txt').writeAsStringSync('x' * 2048);
    File('${dir.path}/alpha.txt').writeAsStringSync('a');
    final subdir = Directory('${dir.path}/subdir')..createSync();

    await tester.pumpWidget(_wrap(FolderContentScreen(
      folder: _folder(dir.path),
      load: _loader(dir.path,
          [File('${dir.path}/zeta.txt'), subdir, File('${dir.path}/alpha.txt')]),
    )));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));

    expect(find.text('alpha.txt'), findsOneWidget);
    expect(find.text('zeta.txt'), findsOneWidget);
    expect(find.text('subdir'), findsOneWidget);

    // Directories sort before files regardless of name.
    final subdirY = tester.getTopLeft(find.text('subdir')).dy;
    final alphaY = tester.getTopLeft(find.text('alpha.txt')).dy;
    expect(subdirY, lessThan(alphaY));

    expect(find.textContaining('KB'), findsOneWidget);
  });

  testWidgets('empty folder shows placeholder', (tester) async {
    final dir = Directory.systemTemp.createTempSync('ferri_empty');
    addTearDown(() => dir.deleteSync(recursive: true));

    await tester.pumpWidget(_wrap(FolderContentScreen(
      folder: _folder(dir.path),
      load: _loader(dir.path, []),
    )));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));

    expect(find.text('Nothing here yet — sync to bring files over'),
        findsOneWidget);
  });

  testWidgets('missing folder shows error card with retry', (tester) async {
    await tester.pumpWidget(_wrap(FolderContentScreen(
      folder: _folder('/nonexistent/path/xyz'),
      load: _loader('/somewhere-else', []),
    )));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));

    expect(find.text('Retry'), findsOneWidget);
    expect(find.byIcon(Icons.error_outline), findsOneWidget);
  });

  testWidgets('tapping a subdirectory drills into it', (tester) async {
    final dir = Directory.systemTemp.createTempSync('ferri_drill');
    addTearDown(() => dir.deleteSync(recursive: true));
    final inner = Directory('${dir.path}/inner')..createSync();
    File('${dir.path}/inner/nested.txt').writeAsStringSync('n');

    var loadedPath = '';
    await tester.pumpWidget(_wrap(FolderContentScreen(
      folder: _folder(dir.path),
      load: (path) async {
        loadedPath = path;
        if (path == dir.path) return [inner];
        if (path == inner.path) {
          return [File('${inner.path}/nested.txt')];
        }
        throw StateError('unexpected $path');
      },
    )));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));

    expect(loadedPath, dir.path);
    await tester.tap(find.text('inner'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));

    expect(loadedPath, inner.path);
    expect(find.text('nested.txt'), findsOneWidget);
    // AppBar shows the current directory name.
    expect(find.widgetWithText(AppBar, 'inner'), findsOneWidget);
  });
}
