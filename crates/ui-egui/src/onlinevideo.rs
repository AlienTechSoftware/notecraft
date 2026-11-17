//! Insert > Online Video (issue #284).
//!
//! A video on the web goes into the page as a link: a line that says it is a video, from
//! which site, and the address. Nothing is fetched or played while the page is open; the
//! video plays only when the user clicks the link, in their own browser. So a page with a
//! video in it opens offline, saves the same in HTML and CMGFM (a link is ordinary page
//! text), and never reaches out to a site on its own.
//!
//! The sites below are recognised by name and their addresses are tidied to the plain
//! watch address. Any other `https` address is still accepted, as a video link from that
//! site, because many sites host video and the user knows what they pasted.

/// A video address that may go into a page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Video {
    /// The site, as shown in the line: "YouTube", "Vimeo", or the host name.
    pub site: String,
    /// The address the link opens.
    pub address: String,
}

/// Reads what the user pasted. `None` when it is not a web address that can be linked to.
pub fn read(pasted: &str) -> Option<Video> {
    let pasted = pasted.trim();
    let (scheme, rest) = pasted.split_once("://")?;
    if !scheme.eq_ignore_ascii_case("https") && !scheme.eq_ignore_ascii_case("http") {
        return None;
    }
    if pasted.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return None;
    }
    let (host_part, path) = rest.split_once('/').map_or((rest, ""), |(h, p)| (h, p));
    // No user names in an address ("https://user@host"), which can hide where it goes.
    if host_part.is_empty() || host_part.contains('@') {
        return None;
    }
    let host = host_part.split(':').next().unwrap_or(host_part).to_ascii_lowercase();
    if !host.contains('.') {
        return None;
    }
    let bare = host.strip_prefix("www.").or_else(|| host.strip_prefix("m.")).unwrap_or(&host).to_string();
    let query = |key: &str| {
        let (_, q) = path.split_once('?')?;
        q.split('&').find_map(|pair| pair.strip_prefix(key).and_then(|v| v.strip_prefix('=')).map(str::to_string))
    };
    let path_only = path.split(['?', '#']).next().unwrap_or("");
    let id_ok = |id: &str| !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    match bare.as_str() {
        "youtube.com" | "youtube-nocookie.com" | "music.youtube.com" => {
            let id = if path_only == "watch" {
                query("v")?
            } else if let Some(rest) =
                path_only.strip_prefix("shorts/").or_else(|| path_only.strip_prefix("embed/")).or_else(|| path_only.strip_prefix("live/"))
            {
                rest.split('/').next().unwrap_or("").to_string()
            } else {
                return Some(Video { site: "YouTube".into(), address: pasted.to_string() });
            };
            id_ok(&id).then(|| Video { site: "YouTube".into(), address: format!("https://www.youtube.com/watch?v={id}") })
        }
        "youtu.be" => {
            let id = path_only.split('/').next().unwrap_or("");
            id_ok(id).then(|| Video { site: "YouTube".into(), address: format!("https://www.youtube.com/watch?v={id}") })
        }
        "vimeo.com" | "player.vimeo.com" => {
            let id = path_only.rsplit('/').find(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()));
            Some(Video { site: "Vimeo".into(), address: id.map_or_else(|| pasted.to_string(), |id| format!("https://vimeo.com/{id}")) })
        }
        "dailymotion.com" | "dai.ly" => Some(Video { site: "Dailymotion".into(), address: pasted.to_string() }),
        "ted.com" => Some(Video { site: "TED".into(), address: pasted.to_string() }),
        _ => Some(Video { site: bare, address: pasted.to_string() }),
    }
}

/// The line put into the page: "Video (YouTube): <address>" as a link to the address.
pub fn line(video: &Video) -> String {
    let words = crate::i18n::format_message_id("ui.online_video_line", &[("site", &video.site), ("address", &video.address)]);
    crate::pageedit::link_markup(&words, &video.address)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_sites_are_named_and_tidied_to_their_watch_address() {
        let yt = Video { site: "YouTube".into(), address: "https://www.youtube.com/watch?v=dQw4w9WgXcQ".into() };
        for pasted in [
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://youtube.com/watch?feature=share&v=dQw4w9WgXcQ",
            "https://youtu.be/dQw4w9WgXcQ?t=42",
            "https://m.youtube.com/shorts/dQw4w9WgXcQ",
            "https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ",
            "  https://www.youtube.com/watch?v=dQw4w9WgXcQ  ",
        ] {
            assert_eq!(read(pasted).as_ref(), Some(&yt), "{pasted}");
        }
        assert_eq!(
            read("https://player.vimeo.com/video/76979871"),
            Some(Video { site: "Vimeo".into(), address: "https://vimeo.com/76979871".into() })
        );
        assert_eq!(read("https://www.ted.com/talks/x").map(|v| v.site), Some("TED".into()));
    }

    #[test]
    fn another_site_is_kept_as_it_was_pasted() {
        let v = read("https://video.example.org/watch/123").expect("any https address");
        assert_eq!(v, Video { site: "video.example.org".into(), address: "https://video.example.org/watch/123".into() });
    }

    #[test]
    fn what_is_not_a_web_address_is_refused() {
        for pasted in [
            "",
            "dQw4w9WgXcQ",
            "javascript:alert(1)",
            "file:///C:/videos/a.mp4",
            "ftp://example.com/a.mp4",
            "https://",
            "https://localhost/a",
            "https://user@example.com/a",
            "https://example.com/a b",
            "https://www.youtube.com/watch?v=bad id",
            "https://youtu.be/",
        ] {
            assert_eq!(read(pasted), None, "{pasted}");
        }
    }

    #[test]
    fn the_line_is_a_link_to_the_video() {
        let v = read("https://youtu.be/dQw4w9WgXcQ").expect("video");
        let markup = line(&v);
        assert!(markup.starts_with('['), "{markup}");
        assert!(markup.ends_with("(https://www.youtube.com/watch?v=dQw4w9WgXcQ)"), "{markup}");
        assert!(markup.contains("YouTube"), "{markup}");
    }
}
