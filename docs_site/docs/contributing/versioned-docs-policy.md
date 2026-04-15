---
sidebar_position: 5
---

# Versioned Docs Policy

## Retention policy

Only **Major** and **Minor** releases (versions of the form `X.Y.0`) receive a
snapshot in the versioned docs. Patch releases (`X.Y.Z` where `Z > 0`) and
pre-release builds (`-beta.x`, `-rc.x`, etc.) do not create a new versioned
snapshot — they are shipped as regular commits without freezing the docs.

At most **10** versioned doc sets are kept live on the documentation site at any
one time. When a new Major/Minor release is cut and the count would exceed 10,
the oldest versioned set is removed automatically by `scripts/post_release.sh`.
See [Release Workflow](./release-workflow.md) for when and how to run it.

| Version format | Versioned docs snapshot? |
|---|---|
| `1.0.0` | ✅ Yes (major) |
| `0.3.0` | ✅ Yes (minor) |
| `0.3.1` | ❌ No (patch) |
| `0.3.0-beta.1` | ❌ No (pre-release) |

## Accessing older release docs

Docs for releases that have aged out of the versioned site are still available
on GitHub. Each release is merged through a `release/vX.Y.Z` branch; the docs
folder for that snapshot lives at:

```
https://github.com/conjunction-crew/ssg-tether-capture/tree/release/vX.Y.Z/docs_site/docs
```

Replace `vX.Y.Z` with the version you need.

## Older Releases

The entries below are maintained automatically by `scripts/post_release.sh`
whenever a versioned doc set is retired from the live site.

{/* OLDER_RELEASES_START */}
_No versioned doc sets have been retired yet. Links will appear here automatically as versions age out._
{/* OLDER_RELEASES_END */}
