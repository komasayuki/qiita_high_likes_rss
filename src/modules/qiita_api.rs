use crate::error::AppError;
use reqwest::blocking::Client;
use reqwest::header::{HeaderValue, ACCEPT};
use reqwest::StatusCode;
use roxmltree::Document;
use serde::Deserialize;
use std::thread::sleep;
use std::time::Duration;

const BASE_LIKES_URL: &str = "https://qiita.com/api/v2/items";
const MAX_RETRIES: usize = 3;
const TIMEOUT_SECS: u64 = 15;
const USER_AGENT: &str = "qiita-feed/0.1 (+https://github.com)";

#[derive(Debug, Clone)]
pub struct QiitaItem {
    pub item_id: Option<String>,
    pub title: String,
    pub link: String,
    pub summary: Option<String>,
    pub published: Option<String>,
    pub updated: Option<String>,
    pub author_name: Option<String>,
    pub likes_count: u32,
}

#[derive(Debug, Deserialize)]
struct LikeEntry {}

pub struct QiitaClient {
    client: Client,
    token: Option<String>,
}

impl QiitaClient {
    pub fn new(token: Option<String>) -> Result<Self, AppError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(TIMEOUT_SECS))
            .user_agent(USER_AGENT)
            .build()
            .map_err(|e| AppError::network(format!("HTTP クライアント作成失敗: {}", e)))?;
        Ok(Self { client, token })
    }

    pub fn fetch_feed(&self, feed_url: &str) -> Result<Vec<QiitaItem>, AppError> {
        let mut attempt = 0;
        loop {
            attempt += 1;
            // Atom feed を取得してパースする
            let response = self
                .client
                .get(feed_url)
                .header(ACCEPT, "application/atom+xml")
                .send();
            match response {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        let body = resp.text().map_err(|e| {
                            AppError::network(format!("Feed 読み込み失敗: {}", e))
                        })?;
                        return parse_feed_xml(&body);
                    }
                    if should_retry(status) && attempt < MAX_RETRIES {
                        let backoff = backoff_duration(attempt);
                        eprintln!(
                            "Feed リトライ: url={} status={} attempt={} backoff={}s",
                            feed_url,
                            status,
                            attempt,
                            backoff.as_secs()
                        );
                        sleep(backoff);
                        continue;
                    }
                    return Err(AppError::network(format!(
                        "Feed 取得失敗: url={} status={} attempt={}",
                        feed_url, status, attempt
                    )));
                }
                Err(e) => {
                    if attempt < MAX_RETRIES {
                        let backoff = backoff_duration(attempt);
                        eprintln!(
                            "Feed リトライ: url={} error={} attempt={} backoff={}s",
                            feed_url,
                            e,
                            attempt,
                            backoff.as_secs()
                        );
                        sleep(backoff);
                        continue;
                    }
                    return Err(AppError::network(format!(
                        "Feed 取得失敗: url={} error={} attempt={}",
                        feed_url, e, attempt
                    )));
                }
            }
        }
    }

    pub fn fetch_likes_count(
        &self,
        item_id: &str,
        per_page: u32,
        max_pages: u32,
    ) -> Result<u32, AppError> {
        let mut total = 0u32;
        for page in 1..=max_pages {
            // すべてのページを辿って likes 数を集計する
            let likes = self.fetch_likes_page(item_id, per_page, page)?;
            total = total.saturating_add(likes.len() as u32);
            if likes.len() < per_page as usize {
                return Ok(total);
            }
        }
        eprintln!(
            "likes が上限に達しました: item_id={} total>={} pages={}",
            item_id,
            total,
            max_pages
        );
        Ok(total)
    }

    fn fetch_likes_page(
        &self,
        item_id: &str,
        per_page: u32,
        page: u32,
    ) -> Result<Vec<LikeEntry>, AppError> {
        let mut attempt = 0;
        let url = format!("{}/{}/likes", BASE_LIKES_URL, item_id);
        loop {
            attempt += 1;
            let mut request = self
                .client
                .get(&url)
                .query(&[("per_page", per_page.to_string()), ("page", page.to_string())]);
            request = request.header(ACCEPT, HeaderValue::from_static("application/json"));
            if let Some(token) = &self.token {
                request = request.bearer_auth(token);
            }
            let response = request.send();
            match response {
                Ok(resp) => {
                    let status = resp.status();
                    if status == StatusCode::UNAUTHORIZED {
                        return Err(AppError::network(
                            "Qiita API が 401 を返しました。QIITA_API_TOKEN を設定してください。",
                        ));
                    }
                    if status.is_success() {
                        let parsed: Vec<LikeEntry> = resp.json().map_err(|e| {
                            AppError::network(format!("likes JSON パース失敗: {}", e))
                        })?;
                        return Ok(parsed);
                    }
                    if should_retry(status) && attempt < MAX_RETRIES {
                        let backoff = backoff_duration(attempt);
                        eprintln!(
                            "likes リトライ: url={} status={} attempt={} backoff={}s",
                            url,
                            status,
                            attempt,
                            backoff.as_secs()
                        );
                        sleep(backoff);
                        continue;
                    }
                    return Err(AppError::network(format!(
                        "likes 取得失敗: url={} status={} attempt={}",
                        url, status, attempt
                    )));
                }
                Err(e) => {
                    if attempt < MAX_RETRIES {
                        let backoff = backoff_duration(attempt);
                        eprintln!(
                            "likes リトライ: url={} error={} attempt={} backoff={}s",
                            url,
                            e,
                            attempt,
                            backoff.as_secs()
                        );
                        sleep(backoff);
                        continue;
                    }
                    return Err(AppError::network(format!(
                        "likes 取得失敗: url={} error={} attempt={}",
                        url, e, attempt
                    )));
                }
            }
        }
    }
}

fn parse_feed_xml(xml: &str) -> Result<Vec<QiitaItem>, AppError> {
    let doc = Document::parse(xml)
        .map_err(|e| AppError::network(format!("Feed XML パース失敗: {}", e)))?;
    let feed = doc
        .descendants()
        .find(|n| n.has_tag_name("feed"))
        .ok_or_else(|| AppError::network("Feed に feed 要素がありません"))?;

    let mut items = Vec::new();
    for entry in feed.children().filter(|n| n.has_tag_name("entry")) {
        let title = match child_text(&entry, "title") {
            Some(v) => v,
            None => {
                eprintln!("entry の title が無いためスキップします");
                continue;
            }
        };
        let link = match entry
            .children()
            .find(|n| n.has_tag_name("link") && n.attribute("rel") == Some("alternate"))
            .and_then(|n| n.attribute("href"))
        {
            Some(v) => v.to_string(),
            None => {
                eprintln!("entry の link が無いためスキップします: title={}", title);
                continue;
            }
        };
        let summary = child_text(&entry, "content");
        let published = child_text(&entry, "published");
        let updated = child_text(&entry, "updated");
        let author_name = entry
            .children()
            .find(|n| n.has_tag_name("author"))
            .and_then(|n| child_text(&n, "name"));
        let item_id = extract_item_id(&link);

        items.push(QiitaItem {
            item_id,
            title,
            link,
            summary,
            published,
            updated,
            author_name,
            likes_count: 0,
        });
    }
    Ok(items)
}

fn child_text(node: &roxmltree::Node<'_, '_>, tag: &str) -> Option<String> {
    node.children()
        .find(|n| n.has_tag_name(tag))
        .and_then(|n| n.text())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn extract_item_id(link: &str) -> Option<String> {
    let marker = "/items/";
    let start = link.find(marker)? + marker.len();
    let mut id_part = &link[start..];
    if let Some(end) = id_part.find('?') {
        id_part = &id_part[..end];
    }
    if let Some(end) = id_part.find('#') {
        id_part = &id_part[..end];
    }
    if id_part.is_empty() {
        return None;
    }
    Some(id_part.to_string())
}

fn should_retry(status: StatusCode) -> bool {
    status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS
}

fn backoff_duration(attempt: usize) -> Duration {
    let secs = 2u64.pow((attempt as u32).saturating_sub(1));
    Duration::from_secs(secs)
}
