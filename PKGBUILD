# Maintainer: AB <gh@hexor.ru>
pkgname=rexec
pkgver=1.0.1.r8.g48635fc
pkgrel=1
pkgdesc="Parallel SSH executor"
url="https://github.com/house-of-vanity/rexec"
license=("WTFPL")
arch=("x86_64")
makedepends=("cargo")

pkgver() {
    (git describe --long --tags || echo "$pkgver") | sed 's/^v//;s/\([^-]*-g\)/r\1/;s/-/./g'
}

build() {
    return 0
}

package() {
    cd ..
    usrdir="$pkgdir/usr"
    mkdir -p $usrdir
    cargo install --no-track --path . --root "$usrdir"
}

