defmodule Amber.Snapshot do
  @moduledoc """
  A captured page, reusable across formats — capture once, emit many
  (Plans.md 10.1/11.5). Created by `Amber.snapshot/2`.
  """
  # In its own file so the compiler resolves this struct before `Amber` (which
  # builds `%Amber.Snapshot{}`) — same-file ordering can't satisfy that.
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
