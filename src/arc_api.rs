pub fn build_news_url(
    tag: Option<&str>,
    limit: Option<u32>,
    offset: Option<u32>,
    platform: Option<&str>,
    fields: &[&str],
) -> String {
    let mut url = String::from("https://api.arcgames.com/v1.0/games/sto/news?");
    if let Some(tag) = tag {
        url.push_str(&format!("tag={}&", tag));
    }
    if let Some(limit) = limit {
        url.push_str(&format!("limit={}&", limit));
    }
    if let Some(offset) = offset {
        url.push_str(&format!("offset={}&", offset));
    }
    for field in fields {
        url.push_str(&format!("field%5B%5D={}&", field));
    }
    if let Some(platform) = platform {
        url.push_str(&format!("platform={}&", platform));
    }
    // Remove trailing '&' if present
    if url.ends_with('&') {
        url.pop();
    }
    url
}
