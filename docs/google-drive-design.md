# Google Drive 接続設計

## 方針

Harbor Transferの配布者が所有する共通GCPプロジェクトや共通OAuth Client IDは使用しない。利用者が自分のGoogle CloudプロジェクトでGoogle Drive APIを有効にし、「デスクトップアプリ」型のOAuthクライアントを発行して`credentials.json`を環境設定から読み込む。

Client Secretは設定ファイルやブックマークへ書き出さず、OS Keychainだけへ保存する。OAuth 2.0 Authorization CodeフローをPKCE（S256）とランダムな`state`で保護し、システムブラウザと一時的な`127.0.0.1`のloopback callbackを使用する。callbackはランダムな空きポートを使い、5分でタイムアウトする。

## 保存する情報

- 環境設定: OAuth Client IDのみ
- ブックマーク: 表示名、`googleDrive`プロトコル、初期パス、ローカル同期ディレクトリなどの非機密情報のみ
- OS Keychain: Client ID、Client Secret、Access Token、Refresh Token、有効期限、Googleアカウントのメールアドレス

Access TokenとRefresh TokenをSQLite、localStorage、ブックマークJSON、ログへ出力してはならない。認証解除ではKeychainのGoogle Drive認証情報だけを削除する。

## 権限

汎用ファイル転送ソフトとして既存ファイルの一覧、取得、更新、移動、削除を行うため、`openid`、`email`、`https://www.googleapis.com/auth/drive`を要求する。限定的な`drive.file`ではHarbor Transferが作成・選択したファイル以外を扱えないため、通常のファイルブラウザー要件を満たさない。環境設定ではこの権限範囲を認証前に明示する。

## 現在のファイル操作

- My Driveルートからのパス解決とページネーション付き一覧
- フォルダ作成
- 通常ファイルのストリーミングダウンロード
- resumable uploadによる新規作成と同名ファイル更新
- 改名、フォルダ間移動、ゴミ箱への削除
- Access Token期限切れ時のRefresh Tokenによる自動更新
- 同一フォルダの同名項目をGoogle Drive file IDで識別し、表示名を変えず個別に選択・操作

Google Docs、Sheets、SlidesはそれぞれDOCX、XLSX、PPTXへ、DrawingsはPDFへ書き出してダウンロードする。Googleネイティブ文書を外部エディタから元形式へ安全に上書きすることはできないため、ダブルクリック時もエクスポート保存を案内する。エクスポート形式の選択、共有ドライブのルート選択、ショートカット、APIレート制限の再試行は次の実装範囲とする。

## 利用者向け設定手順

1. Google Cloud Consoleでプロジェクトを作成する。
2. Google Drive APIを有効にする。
3. Google Auth Platformでアプリ名、サポートメール、対象ユーザー、Drive権限を設定する。Externalのテスト運用では利用するGoogleアカウントをテストユーザーへ追加する。
4. Clientsでアプリケーション種類「デスクトップアプリ」のOAuth Client IDを作成する。
5. Harbor Transferの「環境設定 → Google Drive」にClient IDを貼り付ける。
6. 「Googleアカウントを認証」を押し、ブラウザで同意する。
7. 新規接続でGoogle Driveを選択し、接続またはブックマーク保存を行う。
