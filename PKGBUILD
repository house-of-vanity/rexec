# Maintainer: AB <gh@hexor.ru>
pkgname=rexec
pkgver=1.0.4.r0.g3cf1e79
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

