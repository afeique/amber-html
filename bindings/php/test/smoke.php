<?php

declare(strict_types=1);

// Smoke test for the PHP binding. Run `bindings/php/generate.sh` first, then:
//   php bindings/php/test/smoke.php
// A data: URL keeps it self-contained; PDF/screenshot drive a real browser, so
// set AMBER_CHROMIUM_PATH (or let the pinned Chrome for Testing download once).

require __DIR__ . '/../src/Amber.php';

use Amber\Amber;
use Amber\Format;
use Amber\CaptureException;

$url = 'data:text/html,<html><body><h1>Smoke</h1><p>hello</p></body></html>';

$md = Amber::captureMarkdown($url);
if (!str_contains($md, 'Smoke')) {
    fwrite(STDERR, "markdown missing content\n");
    exit(1);
}

$pdf = Amber::capture($url, Format::PDF);
if (substr($pdf, 0, 4) !== '%PDF') {
    fwrite(STDERR, "not a PDF\n");
    exit(1);
}

$png = Amber::capture($url, Format::SCREENSHOT);
if (substr($png, 1, 3) !== 'PNG') {
    fwrite(STDERR, "not a PNG\n");
    exit(1);
}

$dir = sys_get_temp_dir() . '/amber-php-smoke';
$path = Amber::save($url, Format::HTML, $dir, 'page');
if (!str_ends_with($path, 'page.html') || !file_exists($path)) {
    fwrite(STDERR, "save failed: $path\n");
    exit(1);
}

// Capture once, emit many (Plans.md 10.1/11.1).
$snap = Amber::snapshot($url, [Format::MARKDOWN, Format::PDF]);
if (!str_contains($snap->markdown(), 'Smoke')) {
    fwrite(STDERR, "snapshot markdown missing content\n");
    exit(1);
}
if (substr($snap->render(Format::PDF), 0, 4) !== '%PDF') {
    fwrite(STDERR, "snapshot not a PDF\n");
    exit(1);
}
$snapPath = $snap->save(Format::READABLE, $dir, 'snap');
if (!str_ends_with($snapPath, 'snap.txt') || !file_exists($snapPath)) {
    fwrite(STDERR, "snapshot save failed: $snapPath\n");
    exit(1);
}
$snap->close();

try {
    Amber::captureMarkdown('not a url');
    fwrite(STDERR, "expected an exception for a bad URL\n");
    exit(1);
} catch (CaptureException $e) {
    // expected
}

printf("php smoke OK (markdown %dB, pdf %dB, png %dB)\n", strlen($md), strlen($pdf), strlen($png));
