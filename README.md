# Version Vine

![Version Vine Logo](version-vine-logo.svg)

`version-vine` is a git flow opinionated SemVer generating CLI written in Rust.
It's intended to be simple and fast.

## Installation

Download the binary for your architecture from the [Releases](https://github.com/Freakazoid182/version-vine/releases).

Store it anywhere you prefer

## Usage

Run from any folder which is managed by Git

```
Usage: version-vine [OPTIONS]

Options:
  -a, --app-name <APP_NAME>          Useful for monorepos with multiple versionable applications. Tags and release branches will have to be prefixed with an application name. E.g. tag: `app-1.0.0`, branch: `release/app-1.0.0`
  -b, --build-number <BUILD_NUMBER>  Build number to be included in the SemVer build metadata. Often used when using a build system. When not provided, the git commit count for the branch is used
  -s, --skip-fetch                   Skip fetching (improves performance for local runs, but may result in outdated version information)
  -h, --help                         Print help
  -V, --version                      Print version
```

Based on which git branch is active and latest tag, the appropriate version data will be generated.

E.g. on the `main` branch where the latest tag is `0.4.0`:
```sh
{
  "app_version": "0.4.0+56c1976",
  "container_tag": "0.4.0.56c1976",
  "git_branch": "main",
  "git_rev": "56c1976",
  "rev_count": "10"
}
```

If no tag can be found, a fallback version of `0.0.0` will be taken.

For `release/*` and `hotfix/*` branches, tags are ignored and the version will be taken from the branch name. E.g. for branch `release/1.0.0`, the version will be `1.0.0`.

## Behavior:

| branch      | version source      | version bump | pre release | format                                                                                                | notes                        |
| ----------- | ------------------- | ------------ | ----------- | ----------------------------------------------------------------------------------------------------- | ---------------------------- |
| `main`      | latest tag/fallback | none         | none        | `{major}.{minor}.{patch}+{commit_short_hash}`                                                         | requires tag to be on `HEAD` |
| `develop`   | latest tag/fallback | patch + 1    | beta        | `{major}.{minor}.{patch}-beta.{commit_count/build_number}+{commit_short_hash}`                        |                              |
| `feature/*` | latest tag/fallback | patch + 1    | alpha       | `{major}.{minor}.{patch}-alpha.{commit_count/build_number}.{escaped_branch_name}+{commit_short_hash}` |                              |
| `release/*` | branch name         | none         | rc          | `{major}.{minor}.{patch}-rc.{commit_count/build_number}+{commit_short_hash}`                          | existing tags are ignored    |
| `hotfix/*`  | branch name         | none         | rc          | `{major}.{minor}.{patch}-rc.{commit_count/build_number}+{commit_short_hash}`                          | existing tags are ignored    |

## TODOs

* [ ] Introduce config file `vine-version.json/yaml`
* [ ] Make regex matches for branch types configurable
* [ ] Make configurable whether by default minors or patches are bumped

---
Inspired by tools like [GitVersion](https://github.com/GitTools/GitVersion) and [Dunamai](https://github.com/mtkennerly/dunamai)

Thanks to [@ekeij](https://github.com/ekeij)
