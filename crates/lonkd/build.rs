fn main() {
  println!("cargo:rerun-if-changed=../../web/dist");
  let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
  let index = std::path::Path::new(&manifest_dir).join("../../web/dist/index.html");
  if !index.exists() {
    panic!(
      "web/dist not found - build the web UI first: npm install && npm run build (requires node)"
    );
  }
}
