# Maintainer: AB <gh@hexor.ru>
pkgname=rexec
pkgver=1.0.6
pkgrel=1
pkgdesc="Parallel SSH executor"
url="https://github.com/house-of-vanity/rexec"
license=("WTFPL")
arch=("x86_64")
makedepends=("rustup")

package() {
  install -Dm755 "$startdir/target/x86_64-unknown-linux-musl/release/rexec" "$pkgdir/usr/bin/rexec"
}


