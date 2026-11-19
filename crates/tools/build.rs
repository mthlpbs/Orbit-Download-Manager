fn main() {
    println!("cargo:rerun-if-env-changed=ORBIT_RELEASE_PUBLIC_KEY");
}
