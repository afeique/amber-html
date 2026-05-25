// Smoke test for the Dart binding. Run `bindings/dart/generate.sh` first, then:
//   cd bindings/dart && dart pub get && dart test
// A data: URL keeps it self-contained; PDF/screenshot drive a real browser, so
// set AMBER_CHROMIUM_PATH (or let the pinned Chrome for Testing download once).
import 'dart:typed_data';

import 'package:amber_html/amber.dart';
import 'package:test/test.dart';

const url =
    'data:text/html,<html><body><h1>Smoke</h1><p>hello</p></body></html>';

void main() {
  test('markdown contains content', () {
    expect(captureMarkdown(url), contains('Smoke'));
  });

  test('capture returns binary formats', () {
    final pdf = capture(url, Format.pdf);
    expect(pdf.sublist(0, 4), equals(Uint8List.fromList('%PDF'.codeUnits)));

    final png = capture(url, Format.screenshot);
    expect(png.sublist(0, 4), equals(Uint8List.fromList([0x89, 0x50, 0x4E, 0x47])));
  });

  test('snapshot renders many from one capture', () {
    final snap = Snapshot.capture(url, [Format.markdown, Format.pdf]);
    expect(snap.markdown(), contains('Smoke'));
    expect(snap.render(Format.pdf).sublist(0, 4),
        equals(Uint8List.fromList('%PDF'.codeUnits)));
    snap.close();
  });

  test('bad URL throws', () {
    expect(() => captureMarkdown('not a url'), throwsA(isA<CaptureException>()));
    expect(() => Snapshot.capture('not a url', [Format.markdown]),
        throwsA(isA<CaptureException>()));
  });
}
