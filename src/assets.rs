//! Compile-time embedded display assets (font, backgrounds, logos, opening
//! animation). Baking these into the binary makes `w3p-hwm` a single
//! self-contained file: the release ships one artifact, the installer drops one
//! binary, and there is no `W3P_ASSET_DIR` / on-disk asset tree to keep in sync.

use include_dir::{Dir, include_dir};

static FONT_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/font");
static IMG_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/img");

/// UI font (JetBrains Mono Medium).
pub fn font() -> &'static [u8] {
    FONT_DIR
        .get_file("JetBrainsMono-Medium.ttf")
        .expect("embedded font present at build time")
        .contents()
}

/// Dashboard background template.
pub fn background() -> &'static [u8] {
    img_file("lcdbg.png")
}

/// Header logo shown on the install screen.
pub fn logo() -> &'static [u8] {
    img_file("web3-pi-logo-240x70.png")
}

/// Logo used for the dimmed shutdown screen.
pub fn final_logo() -> &'static [u8] {
    img_file("Web3Pi_logo_0.png")
}

/// Opening-animation frames, ordered by filename (`logo3dNNNN.png`, zero-padded
/// so lexical order is frame order).
pub fn animation_frames() -> Vec<&'static [u8]> {
    let Some(dir) = IMG_DIR.get_dir("3D") else {
        return Vec::new();
    };
    let mut files: Vec<_> = dir.files().collect();
    files.sort_by(|a, b| a.path().cmp(b.path()));
    files.into_iter().map(|f| f.contents()).collect()
}

fn img_file(name: &str) -> &'static [u8] {
    IMG_DIR
        .get_file(name)
        .unwrap_or_else(|| panic!("embedded image {name} present at build time"))
        .contents()
}
