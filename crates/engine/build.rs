fn main() {
    pkg_config::Config::new()
        .atleast_version("3.0")
        .probe("rubberband")
        .expect("librubberband not found (pacman -S rubberband)");
}
