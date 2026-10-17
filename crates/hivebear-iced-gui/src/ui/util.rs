//! Formatting helpers shared by the pages (the web app's `types.ts` helpers).

pub fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".into();
    }
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let b = bytes as f64;
    let i = ((b.ln() / 1024f64.ln()).floor() as usize).min(UNITS.len() - 1);
    format!("{:.1} {}", b / 1024f64.powi(i as i32), UNITS[i])
}

/// "~4.2 GB" style size for a download estimate.
pub fn format_download_size(bytes: u64) -> Option<String> {
    (bytes > 0).then(|| format!("~{}", format_bytes(bytes)))
}

pub fn format_tps(tps: f32) -> String {
    format!("{tps:.1} tok/s")
}

/// 12345 → "12,345".
pub fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// The file name of a model path, which is what the UI calls the model.
pub fn model_display_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Model".into())
}

/// Open a link in the system browser. Off the UI thread: on some desktops
/// the launcher blocks until the browser has started.
pub fn open_url(url: &str) {
    let url = url.to_string();
    std::thread::spawn(move || {
        if let Err(e) = open::that(&url) {
            tracing::warn!("Could not open {url}: {e}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_are_humanised() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(4_500_000_000), "4.2 GB");
    }

    #[test]
    fn thousands_are_grouped() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(131_072), "131,072");
        assert_eq!(thousands(1_234_567), "1,234,567");
    }

    #[test]
    fn display_name_is_the_file_name() {
        assert_eq!(model_display_name("/models/q4.gguf"), "q4.gguf");
        assert_eq!(model_display_name("q4.gguf"), "q4.gguf");
    }
}
