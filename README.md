# qiita_high_likes_rss

Qiita の人気記事 feed を Qiita API の likes 数でフィルタし、Atom フィードとして GitHub Pages に公開する Rust 実装です。

## フィード URL 例
- Project Pages: `https://<owner>.github.io/<repo>/feed.xml`
- User/Org Pages（リポジトリ名が `<owner>.github.io` の場合）: `https://<owner>.github.io/feed.xml`

## 初回セットアップ
1. `gh-pages` ブランチを作成
   - `./scripts/bootstrap_gh_pages.sh` を 1 回実行
2. GitHub Pages の公開元を `gh-pages` ブランチ `/`（root）に設定
   - https://docs.github.com/en/pages/getting-started-with-github-pages/configuring-a-publishing-source-for-your-github-pages-site
3. Actions の `contents: write` 権限を確認
4. Qiita API トークン（必要な場合）
   - `QIITA_API_TOKEN` を GitHub Secrets に設定

## 変更方法
- しきい値（likes）
  - `config/config.yaml` の `min_likes` を変更
  - 環境変数 `MIN_LIKES` で上書き可能
  - GitHub Variables の `MIN_LIKES` を設定すると Actions から優先反映
- 公開 URL（site_url）
  - `config/config.yaml` の `site_url` を変更
  - 環境変数 `SITE_URL` で上書き可能（未設定なら `GITHUB_REPOSITORY` から自動導出）
- Cron
  - `.github/workflows/update-feed.yml` の `cron` 1 行を書き換えるだけで変更可能

## ローカル実行例
```
cargo run --release -- --config config/config.yaml \
  --state ./public/state/articles.json \
  --out ./public/feed.xml \
  --index ./public/index.html \
  --last-build ./public/last_build.txt
```

## トラブルシュート
- schedule は **UTC** で動作し、混雑時に遅延・ドロップすることがあります。
  - https://docs.github.com/en/actions/learn-github-actions/events-that-trigger-workflows#schedule
  - https://docs.github.com/actions/managing-workflow-runs/disabling-and-enabling-a-workflow
- `gh-pages` ブランチが存在しない / Pages が無効 / 公開元が誤っていると公開されません。
- Qiita API が 401 を返す場合は `QIITA_API_TOKEN` を設定してください。

## データソース / 帰属
- Qiita 人気記事 feed: https://qiita.com/popular-items/feed
- Qiita API: https://qiita.com/api/v2/docs

## ライセンス
MIT License
