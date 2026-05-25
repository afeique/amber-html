defmodule Amber do
  @moduledoc """
  Local-first web-page capture (Elixir bindings for AmberHTML, Plans.md 11.5).

      Amber.capture_markdown("https://example.com")
      Amber.capture("https://example.com", :pdf)            # -> binary

      snap = Amber.snapshot("https://example.com", [:markdown, :pdf])
      Amber.Snapshot.markdown(snap)        # one capture, many formats
      Amber.Snapshot.save(snap, :html, "out", "page")

  Formats are atoms: `:html`, `:mhtml`, `:markdown`, `:readable`, `:warc`,
  `:wacz`, `:screenshot`, `:pdf`. Captures run on dirty IO schedulers; failures
  raise an `ErlangError`.
  """

  alias Amber.Native

  @formats %{
    html: 0,
    mhtml: 1,
    markdown: 2,
    readable: 3,
    warc: 4,
    wacz: 5,
    screenshot: 6,
    pdf: 7
  }

  @doc "Resolve a format atom (or integer) to its C-ABI selector."
  def format(atom) when is_atom(atom), do: Map.fetch!(@formats, atom)
  def format(int) when is_integer(int), do: int

  @doc "Capture `url` and return its clean Markdown."
  def capture_markdown(url), do: Native.capture_markdown(url)

  @doc "Capture `url` and return its readable plain text."
  def capture_readable(url), do: Native.capture_readable(url)

  @doc "Capture `url` as `format` and return the encoded bytes (a binary)."
  def capture(url, format), do: Native.capture(url, format(format))

  @doc "Capture `url` as `format`, write it into `dir`, return the written path."
  def save(url, format, dir, name \\ nil), do: Native.save(url, format(format), dir, name)

  @doc "Capture `url` once for `formats`; returns an `Amber.Snapshot` (capture once, emit many)."
  def snapshot(url, formats) when is_list(formats) do
    %Amber.Snapshot{ref: Native.snapshot(url, Enum.map(formats, &format/1))}
  end
end

defmodule Amber.Snapshot do
  @moduledoc "A captured page, reusable across formats (Plans.md 10.1/11.5)."
  defstruct [:ref]

  @doc "Render `format` as encoded bytes (a binary)."
  def render(%__MODULE__{ref: ref}, format),
    do: Amber.Native.snapshot_render(ref, Amber.format(format))

  @doc "Render `format` as UTF-8 text."
  def text(%__MODULE__{ref: ref}, format),
    do: Amber.Native.snapshot_text(ref, Amber.format(format))

  @doc "Write `format` into `dir`; returns the written path."
  def save(%__MODULE__{ref: ref}, format, dir, name \\ nil),
    do: Amber.Native.snapshot_save(ref, Amber.format(format), dir, name)

  @doc "The captured page's clean Markdown."
  def markdown(snap), do: text(snap, :markdown)

  @doc "The captured page's readable plain text."
  def readable(snap), do: text(snap, :readable)
end
