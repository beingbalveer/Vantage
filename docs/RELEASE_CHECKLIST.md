# Post-release smoke checklist

Run this after a `v*` tag is published, on a clean macOS 13+ machine or a fresh
user account. Download the assets from the GitHub release instead of a local
build, and use a disposable repository with a disposable remote for the write
flows. Never test destructive actions against a production clone.

The release policy lives in
[`release.yml`](../.github/workflows/release.yml); the state file layout is
described in [`ARCHITECTURE.md`](ARCHITECTURE.md#state).

## Artifacts, signing and notarization

Download `Vantage_<version>_universal.dmg`, the matching `.app.zip` and
`SHA256SUMS` from the GitHub release, then:

```bash
shasum -a 256 -c SHA256SUMS

hdiutil attach -nobrowse -readonly Vantage_<version>_universal.dmg
cp -R /Volumes/Vantage/Vantage.app /Applications/
hdiutil detach /Volumes/Vantage

lipo -archs /Applications/Vantage.app/Contents/MacOS/vantage
codesign --verify --deep --strict --verbose=2 /Applications/Vantage.app
codesign -dv --verbose=4 /Applications/Vantage.app 2>&1 | grep "Authority=Developer ID Application"
spctl --assess --type execute --verbose=2 /Applications/Vantage.app
xcrun stapler validate /Applications/Vantage.app
xcrun stapler validate Vantage_<version>_universal.dmg
```

- [ ] `SHA256SUMS` passes and `lipo` lists both `x86_64` and `arm64`.
- [ ] `codesign` verifies the bundle and reports a Developer ID Application
      authority.
- [ ] `spctl` accepts the app with a `Notarized Developer ID` source.
- [ ] `stapler validate` passes for both the installed app and the downloaded
      DMG.
- [ ] The release is not a preview: the title has no `preview` marker, it is
      not marked prerelease, and the assets include the DMG, zip, `SHA256SUMS`
      and `vantage.rb`. If `ALLOW_ADHOC_RELEASE=true` was used, stop: the build
      is not a supported release.
- [ ] The cask-sync commit is on `main`, and `Casks/vantage.rb` matches the
      released version and DMG checksum.

## App smoke test

- [ ] **Launch.** Open `/Applications/Vantage.app`. It starts without a
      Gatekeeper block and needs no right-click or `xattr` override. The
      window renders, and `Settings → About` shows the tagged version,
      architecture, Git version and state path.
- [ ] **Add a repository.** Use the toolbar `+` or `File → Add Repository…`,
      add the disposable repository, and confirm its row, group, branch and
      status. Quit and relaunch; the repository is still listed.
- [ ] **Stage and commit.** Edit a tracked file, tick it in Changes and check
      the button count. Commit with a subject and description.
      `git log -1 --format=%B` contains both, the row clears, and the commit
      appears without a manual refresh. Unstage a file and confirm it returns
      to the unstaged list.
- [ ] **Amend guard.** In a repository with a local-only tip, right-click the
      tip commit and choose `Amend commit`. The `Amending the last commit`
      banner and pre-filled message appear. Press `Cancel`; `git log -1` is
      unchanged. Re-enter amend, switch to another repository, and confirm
      amend is cancelled or still targets the original repository; it must
      never rewrite the newly selected repository's HEAD. Amend is offered
      only for a local-only tip commit.
- [ ] **Fetch, pull and push.** Using the disposable clone, run
      `File → Fetch Origin` (`Cmd+Shift+F`), `Pull` (`Cmd+Shift+P`) and `Push`
      (`Cmd+P`). Fetch updates ahead/behind without touching the working tree,
      pull fast-forwards, and push publishes the commit. Take the remote
      offline and confirm the failure appears as a Git error, not a success
      toast.
- [ ] **Theme.** Toggle with `View → Toggle Light/Dark Mode` (`Cmd+Shift+L`)
      and set `Settings → Appearance` to System, Light and Dark. Text and
      controls remain readable, and the choice survives relaunch.
- [ ] **Settings and state recovery.** Change default view, sort, auto-fetch
      and window memory; relaunch and confirm the choices persist. Open
      `Settings → Data` and use `Reveal`; the folder contains `state.json`.
      Quit, back up `state.json`, replace it with invalid JSON, and relaunch.
      The app starts with defaults without crashing and leaves
      `state.corrupt-<timestamp>.json` beside it. Restore the backup and
      confirm repositories, groups and preferences return.
