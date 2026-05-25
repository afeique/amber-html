defmodule Amber.Native do
  @moduledoc false
  # Thin NIF module backed by the rustler crate in native/amber_nif (Plans.md
  # 11.5). The stubs below are replaced by the loaded NIF; if loading fails they
  # raise :nif_not_loaded. Use the `Amber` module for the idiomatic API.
  use Rustler, otp_app: :amber, crate: "amber_nif"

  def capture_markdown(_url), do: :erlang.nif_error(:nif_not_loaded)
  def capture_readable(_url), do: :erlang.nif_error(:nif_not_loaded)
  def capture(_url, _format), do: :erlang.nif_error(:nif_not_loaded)
  def save(_url, _format, _dir, _name), do: :erlang.nif_error(:nif_not_loaded)
  def snapshot(_url, _formats), do: :erlang.nif_error(:nif_not_loaded)
  def snapshot_render(_res, _format), do: :erlang.nif_error(:nif_not_loaded)
  def snapshot_text(_res, _format), do: :erlang.nif_error(:nif_not_loaded)
  def snapshot_save(_res, _format, _dir, _name), do: :erlang.nif_error(:nif_not_loaded)
end
