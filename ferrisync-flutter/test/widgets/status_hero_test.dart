import 'package:ferrisync/widgets/dashboard/status_hero.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('stageLabel', () {
    test('narrows raw engine phases to friendly names', () {
      expect(stageLabel('starting'), 'Starting');
      expect(stageLabel('uploading'), 'Uploading');
      expect(stageLabel('downloading'), 'Downloading');
      expect(stageLabel(''), 'Syncing');
      expect(stageLabel('unknown'), 'unknown');
    });
  });

  group('formatEta', () {
    test('formats seconds and minutes', () {
      expect(formatEta(9), '9s');
      expect(formatEta(59), '59s');
      expect(formatEta(60), '1m 00s');
      expect(formatEta(150), '2m 30s');
    });
  });
}
