# Vendored Syphon

Source: https://github.com/Syphon/Syphon-Framework
Commit: `f4761677a45b8034a3c2069ec0f3d2553da81fba` (2026-09-21)
License: BSD-style, see `LICENSE.txt` (must stay with the source).

Upstream ships only an Xcode project, and building its `.metallib` needs
Xcode's Metal toolchain. Vendoring the source lets `swift build` with just the
Command Line Tools produce one self-contained helper binary — no framework to
embed or sign inside the Tauri bundle.

## What's included

The Metal server/client, server discovery and messaging. **Not** included:
the deprecated OpenGL classes (`SyphonOpenGL*`, `SyphonServer`,
`SyphonClient`, `SyphonImage`, `*GL*`, `SyphonIOSurfaceImageCore`) and the
upstream `Syphon.h` umbrella (replaced by a Metal-only one in
`include/Syphon/`).

## Local changes

Search for `SHERPRESENT PATCH`. There is one:

- `SyphonServerRendererMetal.m` compiles its two shaders from source with
  `-[MTLDevice newLibraryWithSource:]` instead of loading the prebuilt
  `default.metallib` from the framework bundle. The embedded source is
  upstream `SyphonMetalShaders.metal` verbatim with `SyphonServerMetalTypes.h`
  inlined; the `.metal` file itself is not vendored.

Upstream's prefix header is kept as `Syphon_Prefix.h` and force-included via
`Package.swift`, as the Xcode project does.

## Updating

1. Copy the same file set from a new upstream commit (see the list above).
2. Re-apply the patch, re-inlining the shader source if it changed.
3. Update the commit hash here.
4. `swift build`, then run the round-trip check in `../../README.md`.
