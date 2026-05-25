//! Elixir/Erlang NIF for AmberHTML via rustler (Plans.md task 11.5).
//!
//! Calls `amber-core`'s Rust API directly — no C marshaling. Every capture runs
//! on a **dirty IO scheduler** because it can block on a real browser, which
//! must never stall a normal BEAM scheduler. The Elixir wrapper (`Amber`) gives
//! these idiomatic names; failures are raised as Erlang error terms.

use amber_core::{snapshot as core_snapshot, CaptureOptions, OutputFormat, Snapshot};
use rustler::{Error, NifResult, ResourceArc};

/// A captured page held across NIF calls — capture once, emit many.
struct SnapshotResource {
    inner: Snapshot,
}

#[rustler::resource_impl]
impl rustler::Resource for SnapshotResource {}

fn to_err<E: std::fmt::Display>(e: E) -> Error {
    Error::Term(Box::new(e.to_string()))
}

/// Map an integer selector (matching `OutputFormat::ALL` order) to a format.
fn format_from_int(i: i64) -> NifResult<OutputFormat> {
    Ok(match i {
        0 => OutputFormat::Html,
        1 => OutputFormat::Mhtml,
        2 => OutputFormat::Markdown,
        3 => OutputFormat::Readable,
        4 => OutputFormat::Warc,
        5 => OutputFormat::Wacz,
        6 => OutputFormat::Screenshot,
        7 => OutputFormat::Pdf,
        _ => return Err(Error::BadArg),
    })
}

#[rustler::nif(schedule = "DirtyIo")]
fn capture_markdown(url: String) -> NifResult<String> {
    capture_text(url, OutputFormat::Markdown)
}

#[rustler::nif(schedule = "DirtyIo")]
fn capture_readable(url: String) -> NifResult<String> {
    capture_text(url, OutputFormat::Readable)
}

fn capture_text(url: String, format: OutputFormat) -> NifResult<String> {
    let snap = core_snapshot(&url, &[format], CaptureOptions::default()).map_err(to_err)?;
    let bytes = snap.render(format).map_err(to_err)?;
    String::from_utf8(bytes).map_err(to_err)
}

/// Capture `url` as `format` and return the encoded bytes (a binary).
#[rustler::nif(schedule = "DirtyIo")]
fn capture(url: String, format: i64) -> NifResult<Vec<u8>> {
    let format = format_from_int(format)?;
    let snap = core_snapshot(&url, &[format], CaptureOptions::default()).map_err(to_err)?;
    snap.render(format).map_err(to_err)
}

/// Capture `url` as `format`, write it into `dir`, return the written path.
#[rustler::nif(schedule = "DirtyIo")]
fn save(url: String, format: i64, dir: String, name: Option<String>) -> NifResult<String> {
    let format = format_from_int(format)?;
    let snap = core_snapshot(&url, &[format], CaptureOptions::default()).map_err(to_err)?;
    let path = snap
        .save(format, std::path::Path::new(&dir), name.as_deref())
        .map_err(to_err)?;
    Ok(path.display().to_string())
}

/// Capture `url` once for `formats`, returning a reusable snapshot resource.
#[rustler::nif(schedule = "DirtyIo")]
fn snapshot(url: String, formats: Vec<i64>) -> NifResult<ResourceArc<SnapshotResource>> {
    let formats: NifResult<Vec<OutputFormat>> = formats.into_iter().map(format_from_int).collect();
    let inner = core_snapshot(&url, &formats?, CaptureOptions::default()).map_err(to_err)?;
    Ok(ResourceArc::new(SnapshotResource { inner }))
}

#[rustler::nif(schedule = "DirtyIo")]
fn snapshot_render(res: ResourceArc<SnapshotResource>, format: i64) -> NifResult<Vec<u8>> {
    res.inner.render(format_from_int(format)?).map_err(to_err)
}

#[rustler::nif(schedule = "DirtyIo")]
fn snapshot_text(res: ResourceArc<SnapshotResource>, format: i64) -> NifResult<String> {
    let bytes = res.inner.render(format_from_int(format)?).map_err(to_err)?;
    String::from_utf8(bytes).map_err(to_err)
}

#[rustler::nif(schedule = "DirtyIo")]
fn snapshot_save(
    res: ResourceArc<SnapshotResource>,
    format: i64,
    dir: String,
    name: Option<String>,
) -> NifResult<String> {
    let path = res
        .inner
        .save(
            format_from_int(format)?,
            std::path::Path::new(&dir),
            name.as_deref(),
        )
        .map_err(to_err)?;
    Ok(path.display().to_string())
}

rustler::init!("Elixir.Amber.Native");
