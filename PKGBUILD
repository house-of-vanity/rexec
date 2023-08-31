# Maintainer: AB <gh@hexor.ru>
pkgname=rexec
pkgver=1.0.6
pkgrel=1
pkgdesc="Parallel SSH executor"
url="https://github.com/house-of-vanity/rexec"
license=("WTFPL")
arch=("x86_64")
makedepends=("rustup")

pkgver() {
    echo "$pkgver" | sed 's/^v//;s/\([^-]*-g\)/r\1/;s/-/./g'
}

build() {
    return 0
}

package() {
    cd ..
    usrdir="$pkgdir/usr"
    mkdir -p $usrdir
    cargo install --target=x86_64-unknown-linux-musl --no-track --path . --root "$usrdir"
}

