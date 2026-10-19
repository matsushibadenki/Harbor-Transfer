# 依存ライブラリ更新記録

## 2026-10-19

前回の未コミット変更（package.json / pnpm-lock.yaml / docs/roadmap.md / この記録 / src/vite-env.d.ts）を保持して、小さな更新を追加した。Rust manifestとCargo.lockは変更していない。

| パッケージ | 今回開始時 | 更新後 |
| --- | --- | --- |
| react / react-dom | 19.3.0 | 19.3.1 |
| lucide-react | 1.55.0 | 1.59.0 |

🟢 [Done] npmのlatestタグ・公式リリースノート・peer dependencies・Node要件・ライセンスを確認。React DOMのReact要件は^19.3.1、LucideはReact 19に対応。ReactはMIT、LucideはISCを維持し、新規のプレリリースは導入していない。Reactの公式変更点はServer Componentsのtransport改善。Lucide 1.56〜1.59のリリースノートを確認し、既存importは移行不要だった。

🟢 [Done] pnpm lint / pnpm build / pnpm checkを実行し成功。Rustのテストランナー上は115 passed、0 failed。前回のRustコンパイル未完了は今回解消した。cargo fmt検査も成功。pnpmのfrozen-lockfileによるオフライン同期確認が成功。既存アプリでimportする51種類のLucideアイコンをReact 19.3.1のrenderToStaticMarkupでSVGへ描画し、すべて成功した。SVG描画検査はネイティブUI検証を代替しない。

⭕️ [Pending] Docker daemonのsocketが引き続き存在せず、pnpm test:integrationは未実行。環境変数によって早期returnするliveテストは実接続成功として数えない。通信ライブラリ・AWS・SSHの更新は保留を維持する。

⭕️ [Pending] Tauriネイティブ画面、英語・日本語・简体中文の操作、ドッキング／フローティング、狭いウインドウ、配布ビルドは今回未検証。今回変更したフロントエンド依存は型検査・ビルド・SVG描画検査を通過した状態でローカルレビュー用に保存した。

⭕️ [Pending] 公式crates.ioを再確認し、ssh-keyの最新stable 0.6.7にppk featureがないことを確認。PPK機能を維持するため=0.7.0-rc.11の既存固定を維持。新たなRCは追加しない。

🟠 [Next] TauriとプラグインはJS/Rustをまとめて更新し、ネイティブ動作を検証する。npmで確認した候補はapi 2.12.4、cli 2.12.3、dialog 2.8.3、fs 2.7.1、process 2.4.0、updater 2.14.0。Rust tauriのstableは2.12.4（MSRV 1.90）。AWSのstable候補はconfig 1.12.0、S3 1.152.0、credential-types 1.3.0（MSRV 1.94.1）。ローカルRust 1.99.0は要件を満たすが、MSRV適合だけでは既存exact固定を変更しない。

公式資料:
- [React 19.3.1](https://github.com/react/react/releases/tag/v19.3.1)
- [Lucide 1.56.0](https://github.com/lucide-icons/lucide/releases/tag/1.56.0)
- [Lucide 1.57.0](https://github.com/lucide-icons/lucide/releases/tag/1.57.0)
- [Lucide 1.58.0](https://github.com/lucide-icons/lucide/releases/tag/1.58.0)
- [Lucide 1.59.0](https://github.com/lucide-icons/lucide/releases/tag/1.59.0)
- [ssh-key stable](https://crates.io/api/v1/crates/ssh-key/0.6.7)

## 2026-10-12

開始時の作業ツリーはクリーン。公式npmレジストリのlatestタグと公式crates.io APIを参照してstable候補を確認した。新規のalpha/beta/rc/nightlyは導入していない。

| パッケージ | 更新前（lockfile） | 更新後 |
| --- | --- | --- |
| react / react-dom | 19.2.8 | 19.3.0 |
| @types/react | 19.2.18 | 19.3.0 |
| @types/react-dom | 19.2.5 | 19.3.0 |
| vite | 7.3.6 | 8.3.4 |
| @vitejs/plugin-react | 4.7.0 | 6.1.2 |
| typescript | 5.9.3 | 7.0.2 |
| lucide-react | 0.577.0 | 1.55.0 |

🟢 [Done] manifestとpnpm-lock.yamlを同期し、更新されたビルドツールの推移的依存も解決。pnpmのsupply-chain policy検査に合格。React DOMのReact peer範囲、LucideのReact 19対応、ReactプラグインのVite 8対応を確認。ライセンスはReact/Vite/plugin-reactがMIT、TypeScriptがApache-2.0、LucideがISC。

🟢 [Done] Vite 8のNode要件（20.19以上の20系、または22.12以上）をpackage.jsonのenginesに記載。検証環境はNode 26.10.0、pnpm 11.25.0、Rust 1.99.0。TypeScript 7でCSS副作用importの型検査が強化されたため、src/vite-env.d.tsでvite/clientを参照。既存のVite設定でビルド成功。Vite 8の既定ターゲットはSafari 16.4等へ変化しており、ネイティブWebViewの回帰確認は保留。

🟢 [Done] pnpm lint、pnpm buildが成功。ビルド成果物は1,923モジュール、JS約464.59 kB（gzip 135.65 kB）。

⭕️ [Pending] Playwright（Browser plugin not available）で最新ビルドのhttp://127.0.0.1:1421を確認したが、Tauri APIがないためmetadata/invokeの初期化エラーで空画面となった。ページタイトルはHarbor Transferだが、画面表示・3言語切替・環境設定操作・狭いウインドウ・アイコン・ドッキング／フローティングを検証できていない。これを更新起因の不具合または成功と断定しない。元から起動していた1420のサーバーは変更せず、検証用1421のサーバーを停止した。

⭕️ [Pending] Docker socketが存在せずdaemonに接続できない。pnpm test:integrationは未実行。環境変数なしで早期returnする実接続テストは、cargo testが成功しても実通信の成功として数えない。

⭕️ [Pending] Tokio 1.53.1→1.53.2 / tokio-util 0.7.19→0.7.20の試行更新は、実通信回帰を行えないため取り消し、Cargo.lockを開始時の状態へ戻した。

⭕️ [Pending] pnpm checkはTypeScript検査を通過したが、cargo testのAWS SDKコンパイルが10分以上続いたため今回の実行を中断（exit 130）。Rustテスト・check全体は未完了であり、成功と扱わない。Rust manifestとlockfileは開始時の状態を維持。pnpm install --lockfile-only --offline --ignore-scripts --frozen-lockfileは成功。

## 未適用のstable候補

公式レジストリ調査時点の候補。まだ移行・機能互換性・各リリースノートの詳細検証を完了していないため、適用可能と断定しない。

| ライブラリ群 | stable候補 | 保留理由 |
| --- | --- | --- |
| Tauri / tauri-build | 2.12.3 / 2.7.2 | JSとRustの整合、ネイティブAPIとパネルの回帰確認が必要。Rust MSRV 1.90。Tauri 3 alphaは除外 |
| dialog / fs / process / updater | 2.8.2 / 2.7.0 / 2.4.0 / 2.13.3 | JS/Rustを一緒に更新しネイティブ動作を確認する |
| AWS config / S3 / credential-types | 1.12.0 / 1.152.0 / 1.3.0 | MSRV 1.94.1はローカルで満たすが、既存のexact固定を変更するには認証・TLS・S3実通信確認が必要 |
| russh / russh-keys / russh-sftp | 0.64.1 / 0.49.2 / 3.0.1 | API移行と鍵認証・ホスト鍵・転送の実接続確認が必要 |
| suppaftp / reqwest / smb2 | 12.2.0 / 0.13.5 / 0.27.2 | FTP/FTPS/WebDAV/SMB実接続確認が必要 |
| keyring / quick-xml / rusqlite | 4.2.0 / 0.42.0 / 0.40.2 | Keychain・XML・DBのメジャー移行と既存データ回帰確認が必要 |
| base64 / rand / sha2 | 0.23.1 / 0.10.3 / 0.11.0 | API移行を分割し、認証・鍵処理の回帰確認が必要 |
| rustls-platform-verifier | 0.7.1 | TLS確認と他の通信ライブラリ更新に合わせて検証 |

⭕️ [Pending] PPK用ssh-keyの最新公開版は0.7.0-rc.11、最新stableは0.6.7。既存のPPK機能を維持するstable代替は確認できず、=0.7.0-rc.11の固定を維持した。機能を失うダウングレードは実施していない。

🟠 [Next] DockerとネイティブTauriを利用できる環境で保留項目を検証し、小さなまとまりで適用する。

公式資料:
- https://react.dev/blog/2026/09/09/react-19-3
- https://vite.dev/guide/migration
- https://github.com/vitejs/vite-plugin-react/releases
- https://github.com/microsoft/TypeScript/releases
- https://github.com/lucide-icons/lucide/releases
- https://crates.io/
