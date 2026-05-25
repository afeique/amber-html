defmodule Amber.MixProject do
  use Mix.Project

  def project do
    [
      app: :amber,
      version: "0.1.0",
      elixir: "~> 1.15",
      description: "Local-first web-page capture engine — Elixir NIF bindings for AmberHTML.",
      package: package(),
      deps: deps()
    ]
  end

  def application do
    [extra_applications: [:logger]]
  end

  defp deps do
    [
      {:rustler, "~> 0.37"},
      {:ex_doc, "~> 0.31", only: :dev, runtime: false}
    ]
  end

  defp package do
    [
      licenses: ["MIT", "Apache-2.0"],
      links: %{"GitHub" => "https://github.com/afeique/amber-html"},
      files: ~w(lib native/amber_nif/src native/amber_nif/Cargo.toml mix.exs README.md)
    ]
  end
end
