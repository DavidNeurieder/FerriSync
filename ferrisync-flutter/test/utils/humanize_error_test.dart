import 'package:ferrisync/utils/humanize_error.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('humanizeError', () {
    test('maps connection failures to a friendly message', () {
      final msg =
          humanizeError('error could not reach 10.0.0.5 — timed out after 5s');
      expect(msg, contains("Couldn't connect to the other device"));
    });

    test('maps certificate/TOFU failures to a re-pair hint', () {
      final msg =
          humanizeError('TOFU verification failed: cert regenerated');
      expect(msg, contains('Re-pair'));
    });

    test('maps hash mismatch to a retry hint', () {
      final msg = humanizeError('hash mismatch for foo.png');
      expect(msg, contains('file'));
    });

    test('falls back for unknown errors', () {
      final msg = humanizeError(Exception('some cryptic internal detail'));
      expect(msg, contains('Something went wrong'));
    });

    test('uses fallback when error is null', () {
      final msg = humanizeError(null, fallback: 'The last sync stopped.');
      expect(msg, contains('The last sync stopped.'));
    });
  });
}