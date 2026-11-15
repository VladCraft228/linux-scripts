fn main() {
    #[cfg(feature = "qt")]
    cxx_qt_build::CxxQtBuilder::new()
        .file("src/bridge.rs")
        .build();
}
