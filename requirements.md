# requirements.md（Codex CLI 用・Rust 実装）
## 目的
GitHub リポジトリ一式を作成する。内容は以下。

- Qiita の「人気記事」feed を定期収集
- **likes 数のしきい値（デフォルト: 10 以上）**でフィルタ
- **Atom 1.0 形式（feed.xml）**を生成
- **GitHub Actions を 1 時間周期**で実行し、成果物を **GitHub Pages**（`gh-pages` ブランチ）で公開

実装言語は **Rust** とする。
しきい値と収集周期（cron）は **変更しやすい**構成にする。

## データソース（HTML スクレイピング禁止）
- Feed: `https://qiita.com/popular-items/feed`
- likes 取得: `GET https://qiita.com/api/v2/items/:item_id/likes`

## 機能要件
### 1) 収集・フィルタ
1. Feed 取得:
   - URL: `https://qiita.com/popular-items/feed`
   - HTTP タイムアウト: 10〜20 秒
   - 一時的失敗（5xx/タイムアウト/ネットワーク）に対して **最大 3 回リトライ**（指数バックオフ）

2. likes 取得:
   - `GET /api/v2/items/:item_id/likes` の配列長を likes 数として使用
   - ページング（`per_page`/`page`）に対応
   - 401 が返る場合は `QIITA_API_TOKEN` の設定を促す

3. フィルタ条件:
   - `likes_count >= MIN_LIKES` を満たす記事のみ残す

### 2) 状態の永続化（実行間で保持）
- 状態ファイル: `state/articles.json`（`gh-pages` ブランチのルート配下）
- 保存内容（最低限）:
  - `item_id` / `link` / `title`
  - `likes_count`
  - `published` / `updated`
  - `last_seen`

保持ポリシー:
- `MAX_STORED_DAYS`（デフォルト 60 日）を超えたものを削除
- `MAX_STORED_ITEMS`（デフォルト 1000 件）を超えたら古い順に削除

### 3) 生成成果物（GitHub Pages で公開されるもの）
必須:
- `feed.xml`（Atom フィード）
- `index.html`（簡易トップ）
- `state/articles.json`（永続状態）
- `last_build.txt`（RFC3339 形式）
- `.nojekyll`

### 4) Atom フィード仕様
Feed レベル（必須）:
- `<feed xmlns="http://www.w3.org/2005/Atom">`
- `id`, `title`, `updated`
- `link rel="self"`（`feed.xml`）
- `link rel="alternate"`（`index.html`）

Entry レベル（必須）:
- `id`, `title`, `link`, `updated`
- `summary type="html"`

並び順:
- likes 降順 → 公開日降順

### 5) GitHub Actions（1 時間周期）
`.github/workflows/update-feed.yml` を作成する。
- `schedule`: 毎時 7 分（UTC）
- `workflow_dispatch`
- `permissions: contents: write`

### 6) 設定
設定ファイル: `config/config.yaml`
- `min_likes: 10`
- `likes_per_page: 100`
- `likes_max_pages: 20`
- `max_feed_entries: 200`
- `max_stored_days: 60`
- `max_stored_items: 1000`
- `site_title`, `site_description`, `site_url`, `feed_path`, `feed_source`

環境変数で上書き:
- `MIN_LIKES`
- `SITE_URL`
- `QIITA_API_TOKEN`

### 7) ブートストラップ
`scripts/bootstrap_gh_pages.sh` を用意する。
- orphan `gh-pages` ブランチ作成
- `.nojekyll`, `index.html`, `state/articles.json`, `last_build.txt`, `feed.xml` を配置

## CLI 仕様
バイナリ名: `qiita-feed`

引数:
- `--config <path>`
- `--state <path>`
- `--out <path>`
- `--index <path>`
- `--last-build <path>`
- `--dry-run`

終了コード:
- `0`: 成功
- `2`: 設定エラー
- `3`: ネットワーク/API 失敗
- `4`: フィード生成失敗
