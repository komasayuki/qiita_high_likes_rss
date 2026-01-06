use clap::Parser;
use chrono::{DateTime, FixedOffset, Utc};
use qiita_high_likes_rss::atom::{build_feed_xml, default_feed_updated, FeedEntry, FeedInfo};
use qiita_high_likes_rss::config::AppConfig;
use qiita_high_likes_rss::error::AppError;
use qiita_high_likes_rss::html::{build_index_html, IndexPage};
use qiita_high_likes_rss::qiita_api::QiitaClient;
use qiita_high_likes_rss::state::{select_updated_time, StateStore, StoredItem};
use std::cmp::Ordering;
use std::fs;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "qiita-feed", version)]
struct Cli {
    #[arg(long)]
    config: PathBuf,
    #[arg(long)]
    state: PathBuf,
    #[arg(long)]
    out: PathBuf,
    #[arg(long)]
    index: PathBuf,
    #[arg(long = "last-build")]
    last_build: PathBuf,
    #[arg(long)]
    dry_run: bool,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{}", err);
        std::process::exit(err.exit_code());
    }
}

fn run() -> Result<(), AppError> {
    let cli = Cli::parse();
    let config = AppConfig::load(&cli.config)?;
    let now = Utc::now();

    let mut state = StateStore::load(&cli.state)?;
    let client = QiitaClient::new(config.qiita_api_token.clone())?;

    // 人気 feed を取得して likes を付与する
    let mut feed_items = client.fetch_feed(&config.feed_source)?;
    let mut enriched = Vec::new();
    for item in feed_items.iter_mut() {
        let Some(item_id) = item.item_id.clone() else {
            eprintln!("item_id が取得できないためスキップ: title={}", item.title);
            continue;
        };
        let likes = client.fetch_likes_count(
            &item_id,
            config.likes_per_page,
            config.likes_max_pages,
        )?;
        item.likes_count = likes;
        if likes >= config.min_likes {
            enriched.push(item.clone());
        }
    }

    let merged = state.merge_from_feed(&enriched, now);
    // 実行間で保持するデータを整理する
    state.prune(now, config.max_stored_days, config.max_stored_items);

    let mut items: Vec<StoredItem> = state
        .items
        .values()
        .cloned()
        .filter(|item| item.likes_count >= config.min_likes)
        .collect();

    // likes 降順 -> 公開日降順で並べる
    items.sort_by(|a, b| compare_items(a, b));
    if items.len() > config.max_feed_entries {
        items.truncate(config.max_feed_entries);
    }

    let site_url = config.site_url.clone();
    let feed_url = build_url(&site_url, &config.feed_path);
    let index_url = if site_url.is_empty() {
        "index.html".to_string()
    } else {
        build_url(&site_url, "index.html")
    };

    let entries = build_entries(&items, now);
    let feed_updated = default_feed_updated(&entries, now);
    let feed_id = if site_url.is_empty() {
        format!("tag:qiita.com,{}:qiita-feed", now.format("%Y"))
    } else {
        feed_url.clone()
    };
    let feed = FeedInfo {
        id: feed_id,
        title: config.site_title.clone(),
        description: config.site_description.clone(),
        updated: feed_updated,
        feed_url: feed_url.clone(),
        index_url: index_url.clone(),
        entries,
    };
    let feed_xml = build_feed_xml(&feed)?;

    let index_page = IndexPage {
        title: config.site_title.clone(),
        description: config.site_description.clone(),
        feed_url,
        updated: feed_updated,
        min_likes: config.min_likes,
        feed_source: config.feed_source.clone(),
    };
    let index_html = build_index_html(&index_page);

    if cli.dry_run {
        println!(
            "dry-run: merged={} stored={} entries={}",
            merged,
            state.items.len(),
            feed.entries.len()
        );
        return Ok(());
    }

    write_output(&cli.out, &feed_xml)?;
    write_output(&cli.index, &index_html)?;
    write_output(&cli.last_build, &now.to_rfc3339())?;
    write_nojekyll(&cli.out)?;
    state.save(&cli.state)?;

    Ok(())
}

fn compare_items(a: &StoredItem, b: &StoredItem) -> Ordering {
    let likes = b.likes_count.cmp(&a.likes_count);
    if likes != Ordering::Equal {
        return likes;
    }

    let a_published = published_time(a);
    let b_published = published_time(b);
    b_published.cmp(&a_published)
}

fn published_time(item: &StoredItem) -> Option<DateTime<FixedOffset>> {
    item.updated
        .as_deref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .or_else(|| {
            item.published
                .as_deref()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        })
}

fn build_entries(items: &[StoredItem], now: DateTime<Utc>) -> Vec<FeedEntry> {
    items
        .iter()
        .filter_map(|item| {
            let updated = select_updated_time(item)
                .or_else(|| published_time(item))
                .unwrap_or_else(|| now.with_timezone(&FixedOffset::east_opt(0).unwrap()));
            let id = build_entry_id(item, now);
            let link = item.link.clone();
            let summary_html = build_summary_html(item);
            Some(FeedEntry {
                id,
                title: item.title.clone(),
                link,
                updated,
                summary_html,
            })
        })
        .collect()
}

fn build_entry_id(item: &StoredItem, now: DateTime<Utc>) -> String {
    if let Some(id) = &item.item_id {
        return format!("tag:qiita.com,{}:{}", now.format("%Y"), id);
    }
    format!("tag:qiita.com,{}:unknown", now.format("%Y"))
}

fn build_summary_html(item: &StoredItem) -> String {
    let likes = format!("Likes: {}", item.likes_count);
    let author = match (&item.author_name, extract_username(&item.link)) {
        (Some(name), Some(username)) => format!(
            "Author: <a href=\"https://qiita.com/{username}\">{name}</a>",
            name = name,
            username = username
        ),
        (Some(name), None) => format!("Author: {}", name),
        _ => "Author: unknown".to_string(),
    };
    let published = item.published.as_deref().unwrap_or("unknown");
    let updated = item.updated.as_deref().unwrap_or("unknown");
    let content = item.summary.as_deref().unwrap_or("(no content)");

    format!(
        "{}<br/>{}<br/>Published: {}<br/>Updated: {}<br/>{}",
        likes, author, published, updated, content
    )
}

fn extract_username(link: &str) -> Option<String> {
    let trimmed = link.split('?').next().unwrap_or(link).trim_end_matches('/');
    let parts: Vec<&str> = trimmed.split('/').collect();
    let items_index = parts.iter().position(|p| *p == "items")?;
    if items_index == 0 {
        return None;
    }
    Some(parts[items_index - 1].to_string())
}

fn build_url(base: &str, path: &str) -> String {
    if base.is_empty() {
        return path.to_string();
    }
    format!("{}/{}", base.trim_end_matches('/'), path.trim_start_matches('/'))
}

fn write_output(path: &PathBuf, content: &str) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| AppError::feed(format!("出力ディレクトリ作成失敗: {}", e)))?;
    }
    fs::write(path, content)
        .map_err(|e| AppError::feed(format!("出力書き込み失敗: {}", e)))?;
    Ok(())
}

fn write_nojekyll(out_path: &PathBuf) -> Result<(), AppError> {
    let Some(parent) = out_path.parent() else {
        return Ok(());
    };
    let nojekyll_path = parent.join(".nojekyll");
    if nojekyll_path.exists() {
        return Ok(());
    }
    fs::write(&nojekyll_path, "")
        .map_err(|e| AppError::feed(format!(".nojekyll 作成失敗: {}", e)))?;
    Ok(())
}
