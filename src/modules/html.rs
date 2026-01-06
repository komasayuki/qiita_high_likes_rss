use chrono::{DateTime, FixedOffset};

pub struct IndexPage {
    pub title: String,
    pub description: String,
    pub feed_url: String,
    pub updated: DateTime<FixedOffset>,
    pub min_likes: u32,
    pub feed_source: String,
}

pub fn build_index_html(page: &IndexPage) -> String {
    format!(
        r#"<!doctype html>
<html lang="ja">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>{title}</title>
  <style>
    body {{ font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; margin: 2rem; line-height: 1.6; }}
    .meta {{ color: #555; }}
  </style>
</head>
<body>
  <h1>{title}</h1>
  <p>{description}</p>
  <p><a href="{feed_url}">feed.xml</a></p>
  <div class="meta">
    <p>最終更新: {updated}</p>
    <p>min_likes: {min_likes}</p>
    <p>source: {feed_source}</p>
  </div>
</body>
</html>
"#,
        title = escape_html(&page.title),
        description = escape_html(&page.description),
        feed_url = escape_html(&page.feed_url),
        updated = page.updated.to_rfc3339(),
        min_likes = page.min_likes,
        feed_source = escape_html(&page.feed_source)
    )
}

fn escape_html(value: &str) -> String {
    // 最低限の HTML エスケープ
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
