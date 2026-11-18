# ManageBIM Model Cache — `.bimc` format, version 1

A compact, fast-loading container for one BIM model: triangulated geometry, colours, spatial
level and **all properties**. Same role as a Navisworks `.nwc`. Typical size: ~10x smaller than the IFC,
loads ~5x faster. Geometry is quantised to **0.1 mm** (lossless for practical purposes).

## File layout
| Offset | Type | Value |
|---|---|---|
| 0 | 4 bytes | `BIMC` |
| 4 | u16 LE | format version (`1`) |
| 6 | u16 LE | flags: `0` = LZ4 block, `1` = DEFLATE |
| 8 | … | compressed payload |

* flags `0`: LZ4 block with the uncompressed size prepended (u32 LE) — written by ManageBIM.
* flags `1`: u32 LE uncompressed size, then raw DEFLATE (RFC 1951) — e.g. .NET `DeflateStream`.

## Payload
All integers are unsigned LEB128 varints; *signed* values are zig-zag encoded (`(v << 1) ^ (v >> 63)`).
Strings are UTF-8.

```
string_count, then string_count × (byte_length, bytes)      -- every string stored once
info_count,    info_count × (key_str, value_str)            -- model information
warning_count, warning_count × (str)
element_count, then for each element:
    name_str, type_str, guid_str, level_str                 -- type = IFC class or category
    hidden_by_default (0/1)
    prop_count, prop_count × (group_str, name_str, value_str)
    vertex_count, triangle_count
    vertex_count × (dx, dy, dz)    signed, delta to the previous vertex, units of 0.1 mm, metres
    triangle_count × (da, db, dc)  signed: a - previous a, b - a, c - a   (vertex indices)
    run_count, run_count × (rgba_u32, triangle_run_length)  colour runs, r | g<<8 | b<<16 | a<<24
```
`*_str` fields are indices into the string table. Coordinates are absolute (project / shared
coordinates) in metres, Z up.

Reference implementations: `src/bimc.rs` (Rust, read + write) and
`revit-addin/BimcWriter.cs` (C#, write).

## Version 2 — streamed chunks (very large models)
Written by the Navisworks exporter: every element is encoded and written as soon as it is read, so memory
stays constant and there is no 4 GB limit. Header: `BIMC`, version `2` (u16), flags `2` (u16), then chunks:

| Bytes | Value |
|---|---|
| 1 | chunk type |
| 4 | u32 LE uncompressed size |
| 4 | u32 LE compressed size |
| … | raw DEFLATE data |

| Type | Content (same varint rules as v1) |
|---|---|
| `1` STRINGS | `count, count × (byte_length, bytes)` — **appended** to the string table |
| `2` ELEMENTS | `count, count × element` — element encoding identical to v1 |
| `3` INFO | `info_count × (key_str, value_str)`, `warning_count × (str)` |
| `0` END | end of file (a missing END = interrupted export) |

A STRINGS chunk always precedes the elements that use its strings. Readers ignore unknown chunk types.
Reference: `navis-addin/BimcStream.cs` (C#, write), `src/bimc.rs` `decode_v2` / `encode_v2` (Rust).

## Version 3 — shared geometry (written by ManageBIM 0.10+)
Same container and payload as version 1 (flags `0` LZ4 or `1` DEFLATE), with one change in each element: after the
properties comes a **geometry tag** (varint):

| Tag | Geometry |
|---|---|
| `0` | own geometry, encoded exactly like v1 (`vertex_count, triangle_count, vertices, triangles`) |
| `2` | own geometry like `0`, and stored as the next **shared geometry** (ids 0, 1, 2… in file order) |
| `1` | `shared_id, dx, dy, dz` — the geometry of `shared_id`, moved by (dx, dy, dz) in units of 0.1 mm |

Then the colour runs, as in v1. Only geometry that is **identical after quantisation** (same vertices relative to the
first one, same triangle indices) is shared, so reading a v3 file gives exactly the same triangles as v1. Name, type,
GUID, level, properties and colours always stay per element. Readers: ManageBIM 0.10+; v1 / v2 remain readable.
