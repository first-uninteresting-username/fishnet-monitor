//! The dashboard's static files, embedded at build time by `build.rs`, so
//! the image needs no extra files.

include!(concat!(env!("OUT_DIR"), "/frontend_files.rs"));

/// A file by its path under `frontend/` (`index.html`, `src/app.js`, …):
/// its bytes and ETag.
pub fn get(path: &str) -> Option<(&'static [u8], &'static str)> {
    FILES
        .iter()
        .find(|(p, _, _)| *p == path)
        .map(|(_, bytes, etag)| (*bytes, *etag))
}

pub fn content_type(path: &str) -> &'static str {
    match path.rsplit_once('.').map(|(_, ext)| ext) {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("woff2") => "font/woff2",
        Some("json") => "application/json",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;

    #[test]
    fn page_and_assets_are_embedded() {
        for path in [
            "index.html",
            "styles.css",
            "favicon.svg",
            "src/app.js",
            "src/live.js",
        ] {
            assert!(get(path).is_some(), "{path} not embedded");
        }
        assert!(get("tailwind.css").is_none());
        assert!(get("vendor.lock").is_none());
    }

    /// Every module in the import map and every relative import in the
    /// embedded scripts resolves to an embedded file.
    #[test]
    fn every_import_resolves() {
        let index = std::str::from_utf8(get("index.html").unwrap().0).unwrap();
        let mapped = Regex::new(r#""\./([^"]+)""#).unwrap();
        for cap in mapped.captures_iter(index) {
            assert!(
                get(&cap[1]).is_some(),
                "import map entry {} missing",
                &cap[1]
            );
        }

        let mut checked = 0;
        let import =
            Regex::new(r#"(?m)(?:^|\s)(?:import|from)\s*\(?\s*"(\.{1,2}/[^"]+)""#).unwrap();
        for (path, bytes, _) in FILES.iter().filter(|(p, _, _)| p.ends_with(".js")) {
            let dir = path.rsplit_once('/').map_or("", |(d, _)| d);
            let text = std::str::from_utf8(bytes).unwrap();
            for cap in import.captures_iter(text) {
                let mut parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
                for seg in cap[1].split('/') {
                    match seg {
                        "." => {}
                        ".." => {
                            parts.pop();
                        }
                        s => parts.push(s),
                    }
                }
                let target = parts.join("/");
                assert!(get(&target).is_some(), "{path} imports missing {target}");
                checked += 1;
            }
        }
        assert!(
            checked > 30,
            "only {checked} imports found; is the regex right?"
        );
    }

    #[test]
    fn content_types() {
        assert_eq!(content_type("index.html"), "text/html; charset=utf-8");
        assert_eq!(
            content_type("vendor/preact.mjs"),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(content_type("vendor/x.woff2"), "font/woff2");
    }
}
