# json

Hand-rolled generic JSON value type, recursive-descent parser, and writer.

Deliberately no `serde` (see the workspace's `docs/dependency-policy.md`).
`JsonValue::Object` is a `Vec<(String, JsonValue)>`, not a map, so unknown
fields round-trip opaquely and in original order — mirroring how the
legacy C# relay treated `out.json` as a generic mutable tree rather than a
typed DTO. This matters when a newer game client writes a field this
crate's own code doesn't know about yet: it must survive a parse/write
round trip rather than being silently dropped.
