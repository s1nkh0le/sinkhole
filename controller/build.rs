use std::path::Path;

// The controller embeds the built web UI (`frontend/dist`) via rust-embed, which
// treats a *missing* folder as a hard compile error (an empty one just embeds
// nothing). The folder is a build artifact: the bundling build paths (RustRover,
// scripts/build_release.sh) run `npm run build` first so the real UI is embedded,
// but every bare `cargo build` does not — CI, `just build`, the Docker image (which
// doesn't copy the frontend), and a fresh `git clone && cargo build`. Rather than
// scatter `mkdir -p frontend/dist` across CI, the justfile, and the Dockerfile, we
// guarantee the folder exists here, once. When the UI hasn't been built it embeds
// nothing and serving no-ops; when it has, that build is left untouched.
fn main() {
    let dist = Path::new("../frontend/dist");
    if !dist.exists() {
        let _ = std::fs::create_dir_all(dist);
    }
    println!("cargo:rerun-if-changed=../frontend/dist");
}
