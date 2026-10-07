fn main() {
    let root = std::path::Path::new("../../vendor/softfloat/source");
    let specialization = root.join("ARM-VFPv2-defaultNaN");
    let mut build = cc::Build::new();
    build
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wno-unused-label")
        .include("reference")
        .include(root.join("include"))
        .include(&specialization)
        .define("SOFTFLOAT_FAST_INT64", None)
        .define("INLINE_LEVEL", "5");
    let mut files: Vec<_> = std::fs::read_dir(root)
        .unwrap()
        .chain(std::fs::read_dir(&specialization).unwrap())
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "c"))
        .collect();
    files.sort();
    build
        .files(files)
        .file("reference/float.c")
        .compile("rekursiv_float_reference");
    println!("cargo:rerun-if-changed=reference");
    println!("cargo:rerun-if-changed=../../vendor/softfloat");
}
