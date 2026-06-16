fn main() {
    pkg_config::Config::new()
        .atleast_version("3.0")
        .probe("rubberband")
        .expect(
            "librubberband not found \
             (Arch: `pacman -S rubberband`; macOS: `brew install rubber-band` \
             then export PKG_CONFIG_PATH=\"$(brew --prefix)/lib/pkgconfig\")",
        );
}
