/// Dart/Flutter bindings for AmberHTML, a local-first web-page capture engine.
/// Wraps the `amber-core` C ABI via `dart:ffi`. See Plans.md (task 11.2).
///
/// Run `generate.sh` first to stage the native library, then:
///
/// ```dart
/// import 'package:amber_html/amber.dart';
///
/// final md = captureMarkdown('https://example.com');
/// final pdf = capture('https://example.com', Format.pdf); // Uint8List
///
/// final snap = Snapshot.capture('https://example.com', [Format.markdown, Format.pdf]);
/// final snapMd = snap.markdown();        // one capture, many formats
/// snap.save(Format.html, 'out', 'page');
/// snap.close();
/// ```
library amber;

import 'dart:ffi';
import 'dart:io';
import 'dart:typed_data';

import 'package:ffi/ffi.dart';

/// Output-format selectors (mirror the C ABI AMBER_FORMAT_*).
enum Format {
  html(0),
  mhtml(1),
  markdown(2),
  readable(3),
  warc(4),
  wacz(5),
  screenshot(6),
  pdf(7);

  const Format(this.value);
  final int value;
}

/// Thrown when a capture fails or an argument is rejected.
class CaptureException implements Exception {
  CaptureException(this.message);
  final String message;
  @override
  String toString() => 'CaptureException: $message';
}

// --- Opaque handle + native signatures ------------------------------------

final class _AmberSnapshot extends Opaque {}

typedef _CaptureTextC = Int32 Function(Pointer<Utf8>, Pointer<Pointer<Utf8>>);
typedef _CaptureTextDart = int Function(Pointer<Utf8>, Pointer<Pointer<Utf8>>);

typedef _CaptureC = Int32 Function(
    Pointer<Utf8>, Int32, Pointer<Pointer<Uint8>>, Pointer<Size>);
typedef _CaptureDart = int Function(
    Pointer<Utf8>, int, Pointer<Pointer<Uint8>>, Pointer<Size>);

typedef _SaveC = Int32 Function(
    Pointer<Utf8>, Int32, Pointer<Utf8>, Pointer<Utf8>, Pointer<Pointer<Utf8>>);
typedef _SaveDart = int Function(
    Pointer<Utf8>, int, Pointer<Utf8>, Pointer<Utf8>, Pointer<Pointer<Utf8>>);

typedef _SnapshotC = Int32 Function(Pointer<Utf8>, Pointer<Int32>, Size,
    Pointer<Pointer<_AmberSnapshot>>);
typedef _SnapshotDart = int Function(
    Pointer<Utf8>, Pointer<Int32>, int, Pointer<Pointer<_AmberSnapshot>>);

typedef _SnapRenderC = Int32 Function(
    Pointer<_AmberSnapshot>, Int32, Pointer<Pointer<Uint8>>, Pointer<Size>);
typedef _SnapRenderDart = int Function(
    Pointer<_AmberSnapshot>, int, Pointer<Pointer<Uint8>>, Pointer<Size>);

typedef _SnapTextC = Int32 Function(
    Pointer<_AmberSnapshot>, Int32, Pointer<Pointer<Utf8>>);
typedef _SnapTextDart = int Function(
    Pointer<_AmberSnapshot>, int, Pointer<Pointer<Utf8>>);

typedef _SnapSaveC = Int32 Function(Pointer<_AmberSnapshot>, Int32,
    Pointer<Utf8>, Pointer<Utf8>, Pointer<Pointer<Utf8>>);
typedef _SnapSaveDart = int Function(Pointer<_AmberSnapshot>, int, Pointer<Utf8>,
    Pointer<Utf8>, Pointer<Pointer<Utf8>>);

typedef _SnapFreeC = Void Function(Pointer<_AmberSnapshot>);
typedef _SnapFreeDart = void Function(Pointer<_AmberSnapshot>);

typedef _StringFreeC = Void Function(Pointer<Utf8>);
typedef _StringFreeDart = void Function(Pointer<Utf8>);

typedef _BytesFreeC = Void Function(Pointer<Uint8>, Size);
typedef _BytesFreeDart = void Function(Pointer<Uint8>, int);

// --- Library loading + bound functions ------------------------------------

final DynamicLibrary _lib = _openLibrary();

DynamicLibrary _openLibrary() {
  final override = Platform.environment['AMBER_LIB'];
  if (override != null && override.isNotEmpty) {
    return DynamicLibrary.open(override);
  }
  final ext = Platform.isMacOS
      ? 'dylib'
      : Platform.isWindows
          ? 'dll'
          : 'so';
  final prefix = Platform.isWindows ? '' : 'lib';
  return DynamicLibrary.open('native/${prefix}amber_core.$ext');
}

final _captureMarkdown =
    _lib.lookupFunction<_CaptureTextC, _CaptureTextDart>('amber_capture_markdown');
final _captureReadable =
    _lib.lookupFunction<_CaptureTextC, _CaptureTextDart>('amber_capture_readable');
final _capture = _lib.lookupFunction<_CaptureC, _CaptureDart>('amber_capture');
final _save = _lib.lookupFunction<_SaveC, _SaveDart>('amber_save');
final _snapshot =
    _lib.lookupFunction<_SnapshotC, _SnapshotDart>('amber_snapshot');
final _snapRender =
    _lib.lookupFunction<_SnapRenderC, _SnapRenderDart>('amber_snapshot_render');
final _snapText =
    _lib.lookupFunction<_SnapTextC, _SnapTextDart>('amber_snapshot_text');
final _snapSave =
    _lib.lookupFunction<_SnapSaveC, _SnapSaveDart>('amber_snapshot_save');
final _snapFree =
    _lib.lookupFunction<_SnapFreeC, _SnapFreeDart>('amber_snapshot_free');
final _stringFree =
    _lib.lookupFunction<_StringFreeC, _StringFreeDart>('amber_string_free');
final _bytesFree =
    _lib.lookupFunction<_BytesFreeC, _BytesFreeDart>('amber_bytes_free');

const int _ok = 0;
const int _errInvalidInput = 1;

CaptureException _fail(int rc) =>
    CaptureException(rc == _errInvalidInput ? 'invalid input' : 'capture failed');

String _callText(
    int Function(Pointer<Utf8>, Pointer<Pointer<Utf8>>) fn, String url) {
  final cUrl = url.toNativeUtf8();
  final out = malloc<Pointer<Utf8>>();
  try {
    final rc = fn(cUrl, out);
    if (rc != _ok) throw _fail(rc);
    final result = out.value.toDartString();
    _stringFree(out.value);
    return result;
  } finally {
    malloc.free(cUrl);
    malloc.free(out);
  }
}

/// Capture [url] and return its clean Markdown.
String captureMarkdown(String url) => _callText(_captureMarkdown, url);

/// Capture [url] and return its readable plain text.
String captureReadable(String url) => _callText(_captureReadable, url);

/// Capture [url] as [format] and return the encoded bytes (binary formats too).
Uint8List capture(String url, Format format) {
  final cUrl = url.toNativeUtf8();
  final out = malloc<Pointer<Uint8>>();
  final len = malloc<Size>();
  try {
    final rc = _capture(cUrl, format.value, out, len);
    if (rc != _ok) throw _fail(rc);
    final bytes = Uint8List.fromList(out.value.asTypedList(len.value));
    _bytesFree(out.value, len.value);
    return bytes;
  } finally {
    malloc.free(cUrl);
    malloc.free(out);
    malloc.free(len);
  }
}

/// Capture [url] as [format], write it into [dir], and return the written path.
/// [name] is the file stem (extension follows the format); null uses a default.
String save(String url, Format format, String dir, [String? name]) {
  final cUrl = url.toNativeUtf8();
  final cDir = dir.toNativeUtf8();
  final Pointer<Utf8> cName = name == null ? nullptr : name.toNativeUtf8();
  final out = malloc<Pointer<Utf8>>();
  try {
    final rc = _save(cUrl, format.value, cDir, cName, out);
    if (rc != _ok) throw _fail(rc);
    final result = out.value.toDartString();
    _stringFree(out.value);
    return result;
  } finally {
    malloc.free(cUrl);
    malloc.free(cDir);
    if (cName != nullptr) malloc.free(cName);
    malloc.free(out);
  }
}

/// A captured page, reusable across many output formats (Plans.md 10.1/11.2).
/// One capture serves every format with no re-fetch and no re-render. Call
/// [close] to release the native handle.
class Snapshot {
  Snapshot._(this._ptr);

  Pointer<_AmberSnapshot> _ptr;

  /// Capture [url] once for [formats]; must be non-empty.
  static Snapshot capture(String url, List<Format> formats) {
    final cUrl = url.toNativeUtf8();
    final n = formats.length;
    final Pointer<Int32> arr = n > 0 ? malloc<Int32>(n) : nullptr;
    for (var i = 0; i < n; i++) {
      arr[i] = formats[i].value;
    }
    final out = malloc<Pointer<_AmberSnapshot>>();
    try {
      final rc = _snapshot(cUrl, arr, n, out);
      if (rc != _ok) throw _fail(rc);
      return Snapshot._(out.value);
    } finally {
      malloc.free(cUrl);
      if (n > 0) malloc.free(arr);
      malloc.free(out);
    }
  }

  /// Render [format] from the captured page as encoded bytes.
  Uint8List render(Format format) {
    final out = malloc<Pointer<Uint8>>();
    final len = malloc<Size>();
    try {
      final rc = _snapRender(_ptr, format.value, out, len);
      if (rc != _ok) throw _fail(rc);
      final bytes = Uint8List.fromList(out.value.asTypedList(len.value));
      _bytesFree(out.value, len.value);
      return bytes;
    } finally {
      malloc.free(out);
      malloc.free(len);
    }
  }

  /// Render [format] from the captured page as UTF-8 text.
  String text(Format format) {
    final out = malloc<Pointer<Utf8>>();
    try {
      final rc = _snapText(_ptr, format.value, out);
      if (rc != _ok) throw _fail(rc);
      final result = out.value.toDartString();
      _stringFree(out.value);
      return result;
    } finally {
      malloc.free(out);
    }
  }

  /// Write [format] into [dir]; returns the written path.
  String save(Format format, String dir, [String? name]) {
    final cDir = dir.toNativeUtf8();
    final Pointer<Utf8> cName = name == null ? nullptr : name.toNativeUtf8();
    final out = malloc<Pointer<Utf8>>();
    try {
      final rc = _snapSave(_ptr, format.value, cDir, cName, out);
      if (rc != _ok) throw _fail(rc);
      final result = out.value.toDartString();
      _stringFree(out.value);
      return result;
    } finally {
      malloc.free(cDir);
      if (cName != nullptr) malloc.free(cName);
      malloc.free(out);
    }
  }

  /// The captured page's clean Markdown.
  String markdown() => text(Format.markdown);

  /// The captured page's readable plain text.
  String readable() => text(Format.readable);

  /// Release the native handle (idempotent).
  void close() {
    if (_ptr != nullptr) {
      _snapFree(_ptr);
      _ptr = nullptr;
    }
  }
}
