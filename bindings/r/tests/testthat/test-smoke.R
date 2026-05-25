# Smoke test for the R binding. Requires the installed package (with the staged
# native lib) and, for PDF/screenshot, a browser — set AMBER_CHROMIUM_PATH or
# let the pinned Chrome for Testing download once.
# A data: URL keeps it self-contained while exercising the real pipeline.

url <- "data:text/html,<html><body><h1>Smoke</h1><p>hello</p></body></html>"

test_that("markdown contains content", {
  expect_true(grepl("Smoke", amber::capture_markdown(url)))
})

test_that("capture returns binary formats", {
  pdf <- amber::capture(url, amber::Format$PDF)
  expect_identical(pdf[1:4], as.raw(c(0x25, 0x50, 0x44, 0x46))) # %PDF
})

test_that("snapshot renders many from one capture", {
  snap <- amber::snapshot(url, c(amber::Format$MARKDOWN, amber::Format$PDF))
  expect_true(grepl("Smoke", snap$markdown()))
  expect_identical(snap$render(amber::Format$PDF)[1:4], as.raw(c(0x25, 0x50, 0x44, 0x46)))
  snap$close()
})

test_that("a bad URL errors", {
  expect_error(amber::capture_markdown("not a url"), "amber")
})
