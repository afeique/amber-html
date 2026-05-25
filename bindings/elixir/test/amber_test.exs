defmodule AmberTest do
  use ExUnit.Case

  # A data: URL keeps it self-contained; PDF/screenshot drive a real browser, so
  # set AMBER_CHROMIUM_PATH (or let the pinned Chrome for Testing download once).
  @url "data:text/html,<html><body><h1>Smoke</h1><p>hello</p></body></html>"

  test "markdown contains content" do
    assert Amber.capture_markdown(@url) =~ "Smoke"
  end

  test "capture returns binary formats" do
    pdf = Amber.capture(@url, :pdf)
    assert binary_part(pdf, 0, 4) == "%PDF"
  end

  test "snapshot renders many from one capture" do
    snap = Amber.snapshot(@url, [:markdown, :pdf])
    assert Amber.Snapshot.markdown(snap) =~ "Smoke"
    assert binary_part(Amber.Snapshot.render(snap, :pdf), 0, 4) == "%PDF"
  end

  test "a bad URL raises" do
    assert_raise ErlangError, fn -> Amber.capture_markdown("not a url") end
  end
end
