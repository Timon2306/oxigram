# AUR packages

- `oxigram-bin`: the prebuilt binary from the release `.deb`.
- `oxigram`: built from the release source with cargo.

To publish or update (needs an AUR account with an SSH key):

```sh
git clone ssh://aur@aur.archlinux.org/oxigram-bin.git
cp packaging/aur/oxigram-bin/PKGBUILD oxigram-bin/
cd oxigram-bin
makepkg --printsrcinfo > .SRCINFO
git add PKGBUILD .SRCINFO && git commit -m "Update to <version>" && git push
```

On a new release: bump `pkgver`, reset `pkgrel=1`, and update `sha256sums`
(the `.deb` sum is in the release's `SHA256SUMS.txt`).
