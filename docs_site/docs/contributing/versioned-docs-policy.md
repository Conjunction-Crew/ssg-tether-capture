---
sidebar_position: 5
---

# Versioned Docs Policy

## Retention policy

Only **Major** and **Minor** releases (versions of the form `X.Y.0`) receive a
**permanent** snapshot in the versioned docs. Patch releases (`X.Y.Z` where
`Z > 0`) and release candidates (`-rc.x`) do not freeze the docs into the
repository. Instead the site always carries two **rolling** versions, built from
the release tags when the docs are deployed:

- **Latest patch**: the newest stable `X.Y.Z` with `Z > 0`. The next patch
  release replaces it. It stays when a newer minor release ships.
- **Latest pre-release**: the newest `X.Y.Z-rc.N` whose `X.Y.Z` is newer than
  every stable release. It is replaced by the next release candidate and
  dropped once `X.Y.Z` itself is released.

Every release gets a docs version: a permanent snapshot, or one of the two
rolling versions. The default version shown to readers is the newest **stable**
one (permanent or latest patch); the pre-release version carries the
"unreleased" banner.

At most **10** permanent doc sets are kept live on the documentation site at any
one time. The two rolling versions come in addition to that cap. When a new
Major/Minor release is cut and the count would exceed 10, the oldest permanent
set is removed automatically by `scripts/prune_versioned_docs.sh`, which the
Release workflow runs right after creating the snapshot.
See [Release Workflow](./release-workflow.md) for how releases are cut and
`scripts/docs_versions.sh plan <version>` for what a given release would publish.

| Version format | Docs version |
|---|---|
| `1.0.0` | ✅ Permanent snapshot (major) |
| `0.3.0` | ✅ Permanent snapshot (minor) |
| `0.3.1` | Rolling *latest patch* (replaced by the next patch) |
| `0.3.0-rc.1` | Rolling *latest pre-release* (dropped when `0.3.0` is released) |

## Accessing older release docs

Docs for releases that have aged out of the versioned site are still available
on GitHub. Every release is tagged `vX.Y.Z`; the docs folder for that snapshot
lives at:

```
https://github.com/conjunction-crew/ssg-tether-capture/tree/vX.Y.Z/docs_site/docs
```

Replace `vX.Y.Z` with the version you need.

## Older Releases

The entries below are maintained automatically by `scripts/prune_versioned_docs.sh`
whenever a versioned doc set is retired from the live site.

{/* OLDER_RELEASES_START */}
_No versioned doc sets have been retired yet. Links will appear here automatically as versions age out._
{/* OLDER_RELEASES_END */}
