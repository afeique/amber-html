#' Output-format selectors (mirror the C ABI AMBER_FORMAT_*).
#' @export
Format <- list(
  HTML = 0L,
  MHTML = 1L,
  MARKDOWN = 2L,
  READABLE = 3L,
  WARC = 4L,
  WACZ = 5L,
  SCREENSHOT = 6L,
  PDF = 7L
)

#' Capture `url` and return its clean Markdown.
#' @export
capture_markdown <- function(url) {
  .Call("r_amber_capture_text", url, TRUE)
}

#' Capture `url` and return its readable plain text.
#' @export
capture_readable <- function(url) {
  .Call("r_amber_capture_text", url, FALSE)
}

#' Capture `url` as `format` and return the encoded bytes (a raw vector).
#' @export
capture <- function(url, format) {
  .Call("r_amber_capture", url, as.integer(format))
}

#' Capture `url` as `format`, write it into `dir`, return the written path.
#' `name` is the file stem (extension follows the format); NULL uses a default.
#' @export
save_capture <- function(url, format, dir, name = NULL) {
  .Call("r_amber_save", url, as.integer(format), dir, name)
}

#' Capture `url` once for `formats`, returning a reusable snapshot — capture
#' once, emit many. Returns an `amber_snapshot` object with `$render(format)`,
#' `$text(format)`, `$save(format, dir, name)`, `$markdown()`, `$readable()`,
#' and `$close()`.
#' @export
snapshot <- function(url, formats) {
  ptr <- .Call("r_amber_snapshot", url, as.integer(formats))
  self <- new.env(parent = emptyenv())
  self$render <- function(format) .Call("r_amber_snapshot_render", ptr, as.integer(format))
  self$text <- function(format) .Call("r_amber_snapshot_text", ptr, as.integer(format))
  self$save <- function(format, dir, name = NULL) {
    .Call("r_amber_snapshot_save", ptr, as.integer(format), dir, name)
  }
  self$markdown <- function() self$text(Format$MARKDOWN)
  self$readable <- function() self$text(Format$READABLE)
  self$close <- function() invisible(.Call("r_amber_snapshot_close", ptr))
  class(self) <- "amber_snapshot"
  self
}
