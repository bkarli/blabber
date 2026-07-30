/// Tauri commands must return `Result<T, String>` - the frontend only ever
/// sees the `Display` of an error, never the type. This collapses the
/// `.map_err(|e| e.to_string())` boilerplate that pattern otherwise repeats
/// at nearly every fallible call in the command layer.
pub(crate) trait ResultExt<T> {
    fn stringify_err(self) -> Result<T, String>;
}

impl<T, E: std::fmt::Display> ResultExt<T> for Result<T, E> {
    fn stringify_err(self) -> Result<T, String> {
        self.map_err(|e| e.to_string())
    }
}
