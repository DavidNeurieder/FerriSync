import 'dart:typed_data';

import 'package:flutter_zxing/flutter_zxing.dart';

/// Encode a pairing address (`ip:port`) as a QR code image.
///
/// `flutter_zxing` enlarges symbols that do not fit the requested box, so the
/// PNG must be built with the dimensions reported by the encoder rather than
/// the requested ones.
Future<Uint8List?> encodePairingQr(String payload) async {
  final result = zx.encodeBarcode(
    contents: payload,
    params: EncodeParams(
      format: Format.qrCode,
      width: 512,
      height: 512,
      margin: 2,
      eccLevel: EccLevel.medium,
    ),
  );
  if (!result.isValid || result.data == null) return null;
  return pngFromBytes(
    result.data!,
    result.width ?? 512,
    result.height ?? 512,
  );
}
