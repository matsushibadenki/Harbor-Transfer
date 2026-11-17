# 依存ライブラリ更新記録

## 2026-11-16

- 🟢 [Done] Vite 8.3.6 → 8.3.8、Reactプラグイン 6.2.1 → 6.3.0、Lucide 1.70.0 → 1.73.0。公式リリースノートとnpmのNode・peer範囲、ライセンスを確認。既存コードの移行は不要。Rolldownも1.2.19へ追随。
- 🟢 [Done] pnpm build、pnpm check（Rustテストランナー上129件成功）、pnpm 10のfrozen-lockfile検証が成功。既存の未コミット変更を保持。
- ⭕️ [Pending] Dockerに接続できず、実接続テストは未実施。ネイティブ画面も未検証のためTauri・通信依存は据え置き。ssh-keyのstable 0.6.7にはppk機能がなく、既存RC固定を維持。

## 2026-11-09

開始時にあった未コミットのmanifest、lockfile、更新記録、roadmapの変更を保持し、以下を追加更新。Rust manifest / Cargo.lockは変更していない。

| パッケージ | 今回開始時 | 更新後 |
| --- | --- | --- |
| vite | 8.3.5 | 8.3.6 |
| @vitejs/plugin-react | 6.1.2 | 6.2.1 |
| lucide-react | 1.66.0 | 1.70.0 |

🟢 [Done] 公式npm latestタグ、ライセンス、Node/peer要件、公式GitHubリリースノートを確認。ViteとReactプラグインはMIT、Node ^20.19.0 || >=22.12.0、プラグインのVite peerは^8.0.0。LucideはISC、React 19対応を維持。ViteはCSS依存収集・開発サーバー・ビルド等の修正。プラグインはrefreshInclude/refreshExclude追加と修正、optional compiler peer範囲の拡張であり、既存のreact()設定は移行不要。Lucide 1.68でwarning/alert系名変更があるが、アプリの既存52種類のimportは全てSVG描画可能だった。Rolldown/bindingsは1.2.16、@oxc-project/typesは0.156.0へ追随。新たなalpha/beta/rc/nightlyは導入していない。

🟢 [Done] pnpm check / pnpm buildが成功。Rustテストランナー上129 passed、0 failed、整形検査成功。52種類のLucideアイコンのReact SVG描画成功。manifestとlockfileは既存caret方針を維持。pnpm supply-chain policy検査成功。実行環境はNode 26.12.1、Rust 1.99.1、pnpm 12.15.0。CI用pnpm 10.34.9でfrozen-lockfile同期確認も成功。

⭕️ [Pending] Docker daemon socketが存在せずpnpm test:integrationは未実行。環境変数なしで早期returnするliveテストは実接続成功として数えない。ネイティブ画面・3言語操作・ドッキング／フローティング・配布ビルドも今回未検証。Tauri／通信ライブラリ／AWS SDKの固定は検証環境が整うまで維持。

⭕️ [Pending] 公式crates.ioを再確認し、ssh-keyのlatest stableは0.6.7、ppk featureなし。PPK機能のstable代替を確認できないため、既存=0.7.0-rc.11固定を維持する。

🟠 [Next] ネイティブTauriとDocker実通信を検証できる環境でTauriのJS/Rustと各プラグインをまとめて更新。SSD空き約29GiBのため、今回の検査は既存キャッシュを使用し、大規模なRust依存移行は保留。

公式資料:
- [Vite 8.3.6](https://github.com/vitejs/vite/releases/tag/v8.3.6)
- [React plugin 6.2.0](https://github.com/vitejs/vite-plugin-react/releases/tag/plugin-react%406.2.0)
- [React plugin 6.2.1](https://github.com/vitejs/vite-plugin-react/releases/tag/plugin-react%406.2.1)
- [Lucide 1.67.0](https://github.com/lucide-icons/lucide/releases/tag/1.67.0)
- [Lucide 1.68.0](https://github.com/lucide-icons/lucide/releases/tag/1.68.0)
- [Lucide 1.69.0](https://github.com/lucide-icons/lucide/releases/tag/1.69.0)
- [Lucide 1.70.0](https://github.com/lucide-icons/lucide/releases/tag/1.70.0)

## 2026-11-02

開始時の未コミット変更（前回のVite/Lucide更新、lockfile、更新記録、roadmap）を保持し、Lucideだけを追加更新。Rust manifestとCargo.lockは変更していない。

| パッケージ | 今回開始時 | 更新後 |
| --- | --- | --- |
| lucide-react | 1.61.0 | 1.66.0 |

🟢 [Done] 公式npmのlatestタグ、React 19対応peer範囲、ISCライセンス、公式GitHubの1.62〜1.66リリースノートとprerelease=falseを確認。per-icon exports追加、DynamicIconの古いロード結果の無視、アイコン追加・修正等を含む。既存の52種類のimportは変更不要で、React renderToStaticMarkupによるSVG描画がすべて成功。新しいalpha/beta/rc/nightlyは導入していない。

🟢 [Done] manifest/lockfileを同期。pnpm checkとpnpm buildが成功（Rustテストランナー上129 passed、0 failed、整形検査成功）。実接続設定のないliveテストの早期returnを実通信成功には数えない。pnpm supply-chain policy検査も成功。

⭕️ [Pending] Docker daemonのsocketが存在しないためpnpm test:integrationは未実行。ネイティブ画面・3言語操作・ドッキング／フローティングの回帰確認も未実施であり、SVG描画の成功で代替しない。Tauriと通信依存の更新は従来どおり保留。npmの候補はapi/cli 2.12.4、dialog 2.8.4、fs 2.7.1、process 2.4.0、updater 2.14.1。

⭕️ [Pending] 公式crates.ioでssh-keyのstableは0.6.7、ppk featureなしと再確認。PPK機能を維持できるstable代替を確認できないため、=0.7.0-rc.11固定を維持。

🟠 [Next] ネイティブと実接続の検証環境が整ったらTauriのJS/Rust・プラグインと通信ライブラリを分割更新。SSD空き容量は実行開始時約23GiB（使用率99%）のため、大きなRust依存の移行や追加ビルド成果物の生成を今回は避け、既存キャッシュで検証した。既存の成果物やユーザーファイルは削除していない。

公式資料:
- [Lucide 1.62.0](https://github.com/lucide-icons/lucide/releases/tag/1.62.0)
- [Lucide 1.63.0](https://github.com/lucide-icons/lucide/releases/tag/1.63.0)
- [Lucide 1.64.0](https://github.com/lucide-icons/lucide/releases/tag/1.64.0)
- [Lucide 1.65.0](https://github.com/lucide-icons/lucide/releases/tag/1.65.0)
- [Lucide 1.66.0](https://github.com/lucide-icons/lucide/releases/tag/1.66.0)

## 2026-10-26

開始時の作業ツリーはクリーン。アプリ0.2.6の既存機能を維持し、ビルドツールとアイコンを更新。Rust manifest / Cargo.lockは変更していない。

| パッケージ | 更新前 | 更新後 |
| --- | --- | --- |
| vite | 8.3.4 | 8.3.5 |
| lucide-react | 1.59.0 | 1.61.0 |

🟢 [Done] 公式npm latestタグと公式リリースノートを確認。Viteはビルド・CSS・開発サーバー修正、Lucide 1.60/1.61は主にアイコン追加。ViteのNode要件は^20.19.0 || >=22.12.0、MITライセンスを維持。LucideはReact 19をpeer範囲に含み、ISCライセンスを維持。既存APIの移行は不要だった。manifestの既存caret方針を維持し、lockfileを同期。Rolldown/bindingsは1.2.13→1.2.15、@oxc-project/typesは0.153.0→0.155.0へ追随し、今回追加したプレリリースはない。

🟢 [Done] pnpm check / pnpm buildが成功。Rustテストランナー上129 passed、0 failed、整形検査も成功。アプリで使用する52種類のLucideアイコンをReactのrenderToStaticMarkupでSVG描画しすべて成功。CIで使用中のpnpm 10.34.9でもinstall --lockfile-only --ignore-scripts --frozen-lockfileが成功。更新操作はpnpm 12.15.0、Node 26.12.1、Rust 1.99.1で実行した。

⭕️ [Pending] Docker daemonのsocketが存在せず、実接続統合テストは未実行。環境変数なしで早期returnするliveテストは実通信成功として数えない。Tauri・通信・AWSの更新は引き続き保留。今回のネイティブ画面・3言語操作・ドッキング／フローティング・配布ビルドは未検証。SVG描画検査はGUI検証を代替しない。

⭕️ [Pending] ssh-keyの公式latest stableは0.6.7でppk featureなし。PPK機能のため既存=0.7.0-rc.11固定を維持し、新たなRCは追加しない。

🟠 [Next] ネイティブ環境とDocker実通信の検証が可能になったらTauriのJS/Rustとプラグインをまとめて更新。確認した候補はRust tauri 2.12.5（MSRV 1.90）、npm api/cli 2.12.4、dialog 2.8.3、fs 2.7.1、process 2.4.0、updater 2.14.0。AWS候補はconfig 1.13.0、S3 1.155.0（MSRV 1.94.1）。ローカルはMSRVを満たすが、固定理由となる認証・TLS・転送互換性の検証前には適用しない。

公式資料:
- [Vite 8.3.5](https://github.com/vitejs/vite/releases/tag/v8.3.5)
- [Lucide 1.60.0](https://github.com/lucide-icons/lucide/releases/tag/1.60.0)
- [Lucide 1.61.0](https://github.com/lucide-icons/lucide/releases/tag/1.61.0)
- [ssh-key stable metadata](https://crates.io/api/v1/crates/ssh-key/0.6.7)

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
