# ファイル転送ソフト ロードマップ

## 方針

各フェーズは動作する最小成果物として完結させる。`r-shell` のコードからのフォークです。

## 進捗

- [Done] 初期機能設計書を作成し、対象プロトコル、画面、セキュリティ、IPC、受け入れ条件を定義
- [Done] ロードマップを作成し、Tauri 2 + Rust を前提に実装順を定義
- [Done] ルートディレクトリに独立した Tauri 2 + Rust アプリを作成し、フロントエンド/バックエンドのビルドを確認
- [Done] SFTP、FTP、Explicit FTPS の接続・一覧・基本ファイル操作・単一ファイル転送を移植
- [Done] ローカル `~/.ssh` の鍵メタデータを安全に一覧・選択するキー・マネージャーを追加
- [Done] SQLite でブックマーク（秘密情報を除く）を永続化
- [Done] 再帰フォルダアップロードとファイル単位の進捗イベントを実装
- [Done] SSH ホスト鍵の初回確認・fingerprint 保存・変更時の接続中止を実装
- [Done] フォルダ転送をファイル境界で停止・再開・取消できる制御トークンを実装
- [Done] パスワードとSSH鍵パスフレーズをmacOS Keychainに保存・自動復元し、SQLiteやブックマーク書き出しへ平文保存しないようにした
- [Done] Keychain資格情報をアプリ起動中に一度だけ読み込み、接続時の重複読込・同値再保存・不要な削除による認証ダイアログの反復表示を防止
- [Done] ブックマークとGoogle Driveの秘密情報を単一のKeychain保管庫へ集約し、項目ごとに繰り返されるmacOS認証要求を削減（既存項目は使用時に段階移行）
- [Done] 失敗済みの単一ファイル転送を転送パネルから再試行可能にした
- [Done] 単一 SFTP ファイル転送をストリーミング化し、バイト単位の進捗・停止/再開/取消を実装
- [Done] Finder ドロップ、競合解決、転送履歴、速度・残り時間、完了通知を実装
- [Done] ブックマークの表示名をホスト名から分離し、接続共通の環境設定画面と永続化基盤を実装
- [Done] Phase 3 の SFTP / FTP / FTPS 共通ファイルシステム抽象化と双方向の同期プレビューを実装
- [Done] 同期プレビューからの安全な実行、除外パターン、競合選択、停止、実行ログを実装
- [Done] Phase 4 の統合テスト、CI、耐障害性、アクセシビリティ、配布準備を完了
- [Done] Phase 5 の HTTPS WebDAV アダプタ、UI統合、標準サーバー／Nextcloud統合テストを完了
- [Done] Phase 6 の S3 安全設計、Keychain 認証、prefix 一覧アダプタ、ストリーミング転送と接続 UI を実装
- [Done] MinIOのS3互換テスト環境でmultipart・取消・Unicode key・1,000件超一覧を統合検証
- [Done] S3向けフォルダアップロード、空フォルダ方針、copy＋delete操作、同期を実装
- [Done] rsync互換のサイズ＋更新日時quick-checkを選択可能な高度差分同期として実装
- [Done] Tauri updaterによる署名付き自動更新、手動確認、進捗表示、再起動適用を英語・日本語・简体中文で実装
- [Done] updater対応版`0.2.0`と`0.2.1`を署名・公証し、GitHub Releasesへ更新メタデータ付きで公開
- [Done] `0.2.2`を署名・公証し、GitHub Releasesへ更新メタデータ付きで公開
- [Done] `0.2.3`でSamba対応、接続設定の保存専用操作、SSH鍵互換性修正を統合し、全プロトコル回帰テストを実施
- [Done] `0.2.4`でGoogle Drive基本操作、表示・並べ替え改善、Keychain認証要求の削減、期限切れ統合テスト証明書の自動更新を統合
- [Done] Rust全ターゲットへClippy警告ゼロ検査を実施し、Samba接続保持による列挙型肥大化を解消
- [Done] Samba（SMB 2/3）を既存の閲覧・ファイル操作・転送・同期機能へ統合し、Phase 8を完了
- [Done] ユーザー所有のGCPプロジェクトとOAuth Client IDを使うGoogle Drive認証、Keychain保存、My Drive基本操作を統合
- [Done] Google Driveの共有アイテム／共有ドライブ、ネイティブGoogle文書の書き出し形式選択、レート制限への再試行を実装し、Phase 9を完了
- [Done] 再接続、再開位置、検証結果、失敗理由を秘密情報除外済みの転送ログへ永続化し、アプリ内表示とJSON書き出しを実装
- [Done] Phase 11の永続キュー、転送再開、検証、再試行、転送ログ、耐障害統合テストを完了
- [Done] 現在の接続先とローカルファイルを左右で参照し、ペインを入れ替えられるデュアルペイン作業領域の基礎を実装
- [Next] 各ペインから別のブックマーク接続を独立して開けるようにする
- [Later] サーバー間転送、高度なSSH認証、再帰検索、Quick Look、追加クラウド、自動化を段階的に実装

## Phase 0 — プロジェクト基盤 [Done]

**完了条件:** ルートディレクトリだけで起動・テストできる Tauri 2 アプリがある。

- [Done] Tauri 2 + Rust + TypeScript フロントエンドをルートに初期化
- [Done] `src-tauri` の Rust モジュール分割、フォーマッタ、lint、ブックマーク永続化のユニットテストを設定
- [Done] 英語・日本語・简体中文を切り替えられる初期 UI を追加
- [Done] macOS ツールバー、空状態、ブックマークサイドバー、リモート一覧、転送パネルを実装
- [Done] 狭いウインドウではアプリ名を維持しつつ、接続・ファイル操作ツールバーをアイコン表示へ切り替えて改行を防止
- [Done] 環境設定のアピアランスから、全表示モードのファイル名を標準（既定）またはボールドへ切り替え可能
- [Done] ファイル表示の行間を最小を含む4段階で変更でき、罫線のない淡いグレー基調へ統一
- [Done] 環境設定から不可視ファイル・フォルダの表示を全表示モードで切り替え可能
- [Done] `r-shell` へのリンク・実行時依存を作らず、ルートプロジェクト単独でビルドを確認

## Phase 1 — ブックマークと SFTP 閲覧 [Done]

**完了条件:** 保存済み SFTP 接続を開き、リモートのファイル一覧を安全に閲覧できる。

- [Done] SQLite にブックマーク、タグ、接続履歴を保存
- [Done] SQLiteをWAL・待機タイムアウト対応にし、Keychain操作や複数ウインドウ後の同時書き込み競合を防止
- [Done] ブックマークの再編集と、バージョン付き JSON による書き出し・読み込みを実装
- [Done] ブックマークをドラッグ＆ドロップまたはキーボードで並べ替え、SQLiteへ順番を永続化
- [Done] ブックマーク並べ替えの内部ドラッグをファイルアップロード用ドロップ監視から分離
- [Done] ブックマークごとに同期候補のローカルディレクトリを選択・保存できる基盤を実装
- [Done] macOS KeychainにパスワードとSSH鍵パスフレーズを保存し、SSH鍵パスだけをブックマークで管理
- [Done] 未暗号化SSH鍵へ古いKeychainパスフレーズが残っていても、その鍵を復号せず正常に読み込むフォールバックを実装
- [Done] 新規接続シート、SSH 公開鍵認証、ホスト鍵確認を実装
- [Done] 新規接続とブックマーク編集で、接続せず設定とKeychain資格情報だけを保存する操作を実装
- [Done] 接続設定の技術入力欄で自動大文字化、スペル訂正、自動補正を無効化
- [Done] SSHキーマネージャーで秘密鍵・公開鍵の個別表示、種別アイコン、ペア表示を実装
- [Done] リモート一覧、パンくず、親ディレクトリ移動、更新、検索を実装
- [Done] リスト・アイコン・Finder 形式のカラム表示と、表示方式の保存を実装
- [Done] 3表示方式をFinder品質へ再設計し、精密な選択・フォーカス状態、長い名前の安全な表示、カラムのファイルプレビュー、項目数／選択数ステータス、ライト／ダーク共通のニュートラル表示面を実装
- [Done] 拡張子別のカラーアイコンと高コントラストなファイル名表示を全表示方式へ実装
- [Done] リスト列の昇順・降順ソート、列幅変更、表示列設定と所有者・グループ・種類の追加列を実装
- [Done] リスト・アイコン・カラム表示の複数選択と共通属性の一括変更を実装
- [Done] 接続設定・一覧パース・ブックマーク・履歴の Rust テストを追加

## Phase 2 — ファイル操作と転送キュー [Done]

**完了条件:** Finder からのアップロードとダウンロードを、可視化されたキューで安全に完了できる。

- [Done] フォルダ作成、改名、削除と確認ダイアログを実装
- [Done] 右クリックメニュー、カット／コピー／ペースト、ダウンロード、SFTP／FTP属性変更を実装
- [Done] Finder ドラッグ&ドロップ、単一/複数/再帰フォルダ転送を実装
- [Done] TransferJob の識別子、進捗・速度・残り時間イベントを実装
- [Done] 停止、再開、取消、失敗時再試行、同名ファイルの上書き/スキップ/別名解決を実装
- [Done] 転送キューの既定折りたたみ、転送状態に応じた自動開閉、ユーザー操作優先制御を実装
- [Done] SQLite の完了・失敗履歴、画面内通知、転送キュー表示を実装
- [Done] 指定した外部エディタでリモートファイルのキャッシュを編集し、保存後に同名ファイルへ安全に上書きする機能を実装
- [Done] 選択したリモートファイル／フォルダを一時キャッシュ経由でFinderやデスクトップへネイティブドラッグコピーする機能を実装

## Phase 3 — FTP / FTPS と同期 [Done]

**完了条件:** FTP/FTPS でも同じファイル操作を提供し、片方向同期をプレビューして実行できる。

- [Done] `RemoteFileSystem` trait を使う SFTP / FTP / Explicit FTPS アダプタを実装
- [Done] FTPS のTLS証明書検証を維持し、FTP / FTPS の実プロトコルを接続状態へ保持
- [Done] ローカル/リモートの再帰スナップショットとサイズ差分を実装
- [Done] ローカル→リモート、リモート→ローカルの片方向同期プレビューを実装
- [Done] 除外パターン、競合ごとのスキップ／転送元優先、安全な同期実行、停止、永続実行ログを実装
- [Done] ブックマークのローカル／リモートディレクトリ設定と、サイズ＋更新日時quick-checkを使うrsync互換差分同期を実装

## Phase 4 — 品質・配布 [Done]

**完了条件:** 日常利用に耐える安定性と配布準備が整っている。

- [Done] Docker 上の SFTP / FTP / Explicit FTPS 統合テスト環境と GitHub Actions CI を整備
- [Done] 再接続、大きなストリーミング転送、Unicode パス、空フォルダの耐障害テストを追加
- [Done] キーボード操作、フォーカス表示、支援技術向けラベル、英語・日本語・简体中文、ライト／ダーク／システム外観を監査・改善
- [Done] 安定したBundle ID、CSP、macOS署名・notarizationスクリプト、更新配信方針、ローカル限定クラッシュ診断を整備
- [Done] WebDAV / S3 を評価し、既存抽象化に適合する WebDAV を次の実装対象に決定

## Phase 5 — WebDAV [Done]

**完了条件:** HTTPS WebDAV 接続で、既存の閲覧・転送・同期機能を安全に利用できる。

- [Done] HTTPS WebDAV の接続設定、Keychain資格情報、`RemoteFileSystem` アダプタを実装
- [Done] PROPFIND、ストリーミングアップロード／ダウンロード、MKCOL、MOVE、DELETE、空コレクションを統合テスト
- [Done] TLS証明書検証を無効化する製品設定を設けず、認証情報をブックマーク／診断へ含めない安全境界を維持
- [Done] 標準仕様に集中したWebDAVテストサーバーとNextcloudの両方で、Unicode／percent encoding、2 MiB超転送、再接続を検証
- [Done] WebDAVを新規接続、ブックマーク、履歴、既定プロトコル、一覧、ファイル操作、転送、同期UIへ統合

## Phase 6 — S3 [Done]

**完了条件:** オブジェクトストレージ固有の制約を明示し、誤削除を避けながら閲覧・転送できる。

- [Done] AWS SDK、認証情報のKeychain保存、リージョン／HTTPSエンドポイント設定、バケット指定の安全設計を確定
- [Done] prefixを仮想ディレクトリとして扱う、ページネーション対応の読み取り専用S3アダプタを実装
- [Done] S3 / S3互換接続を新規接続、ブックマーク、履歴、既定プロトコル、一覧UIへ統合
- [Done] ストリーミングダウンロードと、10,000 part上限に応じてpartサイズを調整するmultipartアップロードを進捗・停止・取消可能な転送キューへ統合
- [Done] 取消・失敗したmultipart uploadのabortと、未完了ダウンロード一時ファイルの削除を実装
- [Done] MinIOのS3互換テスト環境をCIへ追加し、17 MiB超multipart、取消後abort、Unicode key、1,001件一覧を統合検証
- [Done] S3向けフォルダアップロードを、不要なprefixマーカーを作らず転送キューへ統合
- [Done] 空フォルダを0 byte prefix markerで保持するかブックマーク単位で明示設定可能にした
- [Done] 単一objectとprefixのrenameを全copy・サイズ検証後のdeleteとして実装し、5 GB超はmultipart copyへ切り替え
- [Done] root削除拒否、100,000 object上限、確認UIを維持したfile／prefix削除を実装
- [Done] S3を除外パターン、競合保護、停止、履歴を備えた安全な同期プレビュー／実行へ統合

## 次の実装単位

計画済みのPhase 0〜6とrsync互換quick-checkは完了した。今後の新規Phaseを開始する場合も、削除を自動同期へ含めず、プレビュー・競合保護・停止・履歴という既存の安全境界を維持する。
- [Done] 三点リーダー／右クリックのファイル情報画面へ名称変更、パーミッション、更新日時、SFTPの所有者UID・グループGID編集を統合
- [Done] リスト・アイコン・カラム表示で、選択済みの名前またはF2からインライン名称変更できる操作を実装
- [Done] アイコン表示の通常背景を透明化し、最小行間でもファイル名と補助情報が欠けない高さを確保
- [Done] インライン名称変更の成功と一覧再取得を分離し、再取得だけの遅延をTimeoutの操作失敗として誤表示しないよう改善

## Phase 7 — ソフトウェアアップデート [Done]

**完了条件:** 署名検証されたGitHub Releaseをアプリ内で確認・取得し、安全に再起動適用できる。

- [Done] 起動時の自動確認と環境設定からの手動確認を追加
- [Done] バージョン、リリースノート、ダウンロード進捗、エラー、再起動適用UIを3言語で追加
- [Done] Tauri updater/processプラグイン、最小権限、HTTPS endpoint、専用公開鍵を設定
- [Done] リリースワークフローへupdater archive、署名、`latest.json`生成設定を追加
- [Done] FTP統合テストでEPSV probeがデータlistenerを残す問題を修正し、UTF-8 fixtureとPASV port範囲を明示
- [Done] GitHub Actions Secretsへupdater秘密鍵を登録し、`0.2.0`と`0.2.1`を署名付き自動更新対応版として公開
- [Later] 安定版／プレリリース版の更新チャネル分離と段階配信

## Phase 8 — Samba / SMB [Done]

**完了条件:** SMB 2/3共有へ安全に接続し、既存の閲覧・ファイル操作・転送・同期機能を利用できる。

- [Done] 純Rustの`smb2`クライアントを採用し、SMB 1へフォールバックせずSMB 2/3だけを使用する接続設計を確定
- [Done] サーバー、ポート、共有名、初期ディレクトリ、ワークグループ／ドメイン、ゲスト／ユーザー認証をブックマーク設定へ追加
- [Done] パスワードをmacOS Keychainへ保存し、ブックマーク、書き出しファイル、診断ログへ秘密情報を含めない
- [Done] Sambaアダプタを`RemoteFileSystem`へ統合し、一覧、作成、改名、削除、コピー、移動を既存の共通操作へ接続
- [Done] ストリーミングアップロード／ダウンロード、バイト単位の進捗・停止・取消、再帰フォルダ転送、再試行を転送キューへ統合
- [Done] Finderドラッグ＆ドロップ、外部エディタ、一方向同期、競合保護、履歴をSamba接続へ統合
- [Done] 署名済みSMBセッションの`SET_INFO (FileBasicInformation)`でファイル／フォルダの更新日時変更を実装
- [Done] Docker Samba共有でUnicode名、空フォルダ、2 MiB超転送、取消時の部分ファイル削除、権限拒否、切断後の再接続を統合検証
- [Later] Kerberos、DFS、Bonjourによる共有検出、macOS Keychainのネットワークパスワードとの連携を評価

## Phase 9 — Google Drive [Done]

**完了条件:** Google Drive APIへ安全に認証し、マイドライブと共有ドライブで閲覧・転送・基本ファイル操作を利用できる。

- [Done] OAuth 2.0 Authorization Code + PKCE（S256）、ランダムな`state`、loopback redirectの認証設計を実装
- [Done] Access TokenとRefresh TokenをmacOS Keychainへ保存し、ブックマークと書き出しデータから除外
- [Done] ユーザー自身によるGCPプロジェクト作成、Drive API有効化、同意画面、Desktop Client ID発行を支援する環境設定UIを3言語で追加
- [Done] マイドライブに加えて共有アイテムと共有ドライブを選択できる接続設定を追加
- [Done] Google Driveのfile IDを基準にしたアダプタを`RemoteFileSystem`へ統合し、同一フォルダの同名項目を表示名を変えず個別に選択・操作できるパス変換を実装
- [Done] 一覧のページネーション、フォルダ作成、改名、移動、ゴミ箱への削除、通常ファイルのダウンロードを実装
- [Done] resumable uploadと既存の転送キューへの基本統合を実装
- [Done] 転送の停止・取消、APIレート制限の上限付き指数バックオフ、8 MiB単位の再開可能なchunk uploadをGoogle Drive向けに追加
- [Done] Google Docs／Sheets／SlidesをDOCX／XLSX／PPTXへ、DrawingsをPDFへ安全にエクスポートしてダウンロード可能にする
- [Done] Google Driveの一覧取得済みファイルIDを再利用し、深いフォルダを開く際の階層ごとのAPI再照会を削減
- [Done] Google Drive内部IDを含む実パスと表示・編集用パスを分離し、フォルダ移動後も親階層を保持して表示
- [Done] リスト表示の全列を実幅でリサイズ可能にし、ファイル名列と余白の自動伸長を分離
- [Done] Tauriのファイルドロップと競合するHTML5方式を廃止し、Pointer Eventsによるブックマーク順序入れ替えとSQLite保存を実装
- [Done] サイドメニューとファイル領域の区切りをドラッグして幅を変更し、設定をウインドウ間で保存可能にする
- [Done] ファイル選択だけではFinderドラッグ用キャッシュを作らず、実際のドラッグ開始時だけ転送準備を行う
- [Done] Google Docs／Sheets／Slides／Drawingsのエクスポート形式選択と、再アップロード時に別ファイルになる制約の詳細UIを3言語で追加
- [Later] 共有ドライブ権限、ショートカット、容量制限、変更競合を含む実アカウント統合テストを追加
- [Later] 一方向同期へ統合し、自動削除を行わず、実行前プレビューと競合保護を維持

## Phase 10 — Google Cloud FTP [Done]

**完了条件:** Cloud StorageをバックエンドとするGoogle Cloud FTPへ、安全なSFTP接続として接続し、サービス固有の制約を誤操作なく扱える。

- [Done] `cloudFtp`を通常SFTPと区別できる接続種別として、新規接続、ブックマーク、履歴、書き出し／読み込み、既定プロトコルへ追加
- [Done] 既存SFTP転送エンジン、ホスト鍵検証、SSH秘密鍵、Keychainパスフレーズ処理を再利用
- [Done] Cloud FTPでSSH公開鍵認証を必須にし、秘密鍵未選択とパスワード認証をUI／Rustの両方で拒否
- [Done] 一覧、アップロード、ダウンロード、フォルダ操作、外部エディタ、Finderドラッグ、転送キュー、一方向同期へ統合
- [Done] Cloud Storage IAMで管理されるパーミッション、所有者、グループ、更新日時の変更をUIで無効化し、Rust側でも拒否
- [Done] 日本語、英語、简体中文でCloud FTPの認証方式と制約を案内
- [Later] ユーザー所有のGoogle Cloud環境を使うopt-in実接続テストと、Hierarchical Namespace有効／無効バケットのディレクトリrename検証

## 競合比較を踏まえた次期方針

Cyberduck、Transmit、ForkLift、FileZilla Pro、WinSCPとの比較では、Harbor TransferはSFTP、FTP、Explicit FTPS、HTTPS WebDAV、S3、SMB、Google Drive、Google Cloud FTPという接続方式と、安全な片方向同期、macOS Keychain、Finderドラッグ、外部エディタ連携をすでに備えている。一方、成熟製品との差はプロトコル数よりも、通信切断からの実転送再開、デュアルペイン、チェックサム検証、高度なSSH設定、検索・プレビュー、自動化に集中している。

実装順は `Phase 9完了 → Phase 11 Reliable Transfers → Phase 12 Dual-Pane Workspace → Phase 13 Search, Preview and Batch Tools → Phase 14 Advanced SSH and Authentication → Phase 15 Cloud Services → Phase 16 Automation and Security` とする。新しい接続方式を増やす前に、現在の転送エンジンを長時間・大容量運用に耐える状態へ引き上げる。

参考:

- [Cyberduck File Transfers](https://docs.cyberduck.io/cyberduck/transfer/)
- [Cyberduck Browser](https://docs.cyberduck.io/cyberduck/browser/)
- [Transmit](https://www.panic.com/transmit/)
- [ForkLift Manual](https://binarynights.com/manual)
- [WinSCP Synchronize](https://winscp.net/eng/docs/ui_synchronize)
- [FileZilla Pro Protocols](https://filezillapro.com/docs/v3/basic-usage-instructions/filezilla-pro-supported-protocols/)

## Phase 11 — Reliable Transfers [Done]

**完了条件:** 通信切断やアプリ再起動が発生しても転送状態を失わず、対応プロトコルでは転送済み範囲から安全に再開し、転送結果を検証できる。

- [Done] 実行中・停止中・失敗中のファイル／フォルダ転送、競合方針、4 MiB単位の進捗、再試行情報をSQLiteへ保存し、アプリ再起動後に中断理由付きで復元
- [Done] FTP／FTPSのREST、SFTPのseek、WebDAVのRange対応を能力判定し、保存済み位置から転送を再開。非対応のFTP／WebDAV操作は破損を避けて先頭から安全に再実行
- [Done] S3 multipart upload ID、partサイズ、ローカル更新時刻、完了済みpartとETagをSQLiteへ保存。再起動後はListPartsでS3側の状態を照合して継続し、入力変更・期限切れsessionは安全に再作成、明示的キャンセル時はabort
- [Done] Google Drive resumable upload session URLをmacOS Keychain Vaultへ、進捗をSQLiteへ保存。再起動後は308 Rangeで実offsetを照合し、完了済み・410 Gone・入力変更を判定して安全に継続または新規sessionへ切り替え
- [Done] WebDAVは同一collectionの隠し一時resourceへPUTし、PROPFINDのサーバー報告サイズを検証して`MOVE Overwrite`で原子的に置換。単一ファイルとフォルダ内ファイルの両経路へ適用し、失敗時は元ファイルを維持。S3 multipartとGoogle Drive resumableはサービス側の完了操作まで新内容を非公開にする方式を継続（SFTP v3、FTP／FTPS、現行SMBライブラリは既存ファイルの原子的置換を保証できないため従来動作を維持）
- [Done] S3アップロードはローカルSHA-256をresumable sessionとobject metadataへ保存し、完了後にリモートobjectをストリーミング再読込して独立計算・照合。S3ダウンロードも一時ファイルとリモート再読込のSHA-256一致後だけ置換。その他のプロトコルはサーバー報告サイズ（取得不能時は転送APIの確定byte数）を必須検証する能力別フォールバックとし、検証方式を英語・日本語・简体中文の転送履歴へ記録
- [Done] 一時的な通信障害、HTTP 429／5xx、サーバー切断を判別し、単一ファイルのアップロード／ダウンロードとフォルダアップロードを最大3回（500 ms、1 s、2 s）の指数バックオフで自動再試行。FTP／FTPS、SFTP、WebDAV、SMBは保存済みの接続設定から再接続し、S3／Google DriveはSDK・HTTP clientの接続poolとresumable stateを継続利用。認証、権限、パス不正、容量、取消、サイズ／checksum不一致は再試行せず即時停止し、累積再試行回数・理由・再開位置をSQLiteへ保存。再接続中の状態を英語・日本語・简体中文の転送キューへ表示
- [Done] 環境設定でアプリ全体の同時転送数（1〜16）、集約帯域上限（KB/s、0は無制限）、自動再試行回数（0〜10）を指定し、ブックマークごとに継承または上書き可能にする。待機ジョブをSQLiteへ`Queued`として保存し、英語・日本語・简体中文の転送キューへ表示。共通／接続別カウンターを持つ上限制御と、全転送・接続単位で予約を共有するleaky-bucket型帯域制御をバックエンドで再検証し、FTP／FTPS、SFTP、SMBは転送ごとの独立session、WebDAV、S3、Google Driveはclone可能なHTTP／SDK clientを使って別ブックマーク間および同一ブックマーク内の安全な並行転送に対応
- [Done] 失敗理由、再接続、再開位置、検証結果を含む秘密情報除外済み転送ログを表示・JSON書き出し可能にする。URL userinfo、パスワード／パスフレーズ、OAuth token、Client Secret、S3署名、Authorization値はSQLite保存前に伏せ、最大5,000イベントを保持
- [Done] SFTP 16 MiB／WebDAV 8 MiB／S3 multipartの大容量転送、通信中断後の再接続と部分再開、SQLiteを閉じて再度開くアプリ強制終了相当、書き込み失敗・サイズ不一致時の既存保存先維持、S3 SHA-256不一致を統合／障害注入テストへ追加。FTP、Explicit FTPS、SFTP、HTTPS WebDAV、Nextcloud、S3、SMBのDocker実サーバー試験を通過

## Phase 12 — Dual-Pane Workspace [Next]

**完了条件:** 左右のペインをローカルまたは任意の接続先として開き、比較しながら安全に転送できる。

- [Done] 現在のリモートとローカルを左右に並べ、各ペインの接続種別を切り替えて左右を入れ替え可能にする。ローカル側はホーム／ブックマーク指定フォルダを起点に、パス入力、親階層、フォルダ選択、検索、不可視ファイル設定、独立スクロールへ対応
- [Done] 狭いウインドウではデュアルペインを上下配置へ切り替え、各ペインの最低操作領域を維持
- [Next] 左右それぞれから別のブックマーク接続を独立して開き、接続状態とパスを保持できるようにする
- [Later] ローカル↔リモート、ローカル↔ローカル、リモート↔リモートのドラッグ、コピー、移動を共通操作へ統合
- [Later] 異なる接続先間の転送を一時キャッシュ経由で行い、将来は対応プロトコルでserver-side copyへ最適化
- [Later] 左右の同名フォルダをサイズ、更新日時、checksumで比較し、差分だけを選択して転送可能にする
- [Later] 共通の相対パスを維持する連動ナビゲーションを追加
- [Later] 複数タブ、タブ名、接続状態、パス、表示方式、列幅をワークスペースとして復元
- [Later] ローカルとリモートのよく使う場所を登録するPlacesバーを追加

## Phase 13 — Search, Preview and Batch Tools [Later]

**完了条件:** 大きな階層から目的の項目を見つけ、内容を安全に確認し、複数項目を一括処理できる。

- [Later] キャンセル、進捗、件数上限、シンボリックリンク非追跡を備えた再帰ファイル名検索を実装
- [Later] Google Drive、S3など検索APIを持つ接続先ではserver-side searchを利用し、その他は制限付き再帰走査へフォールバック
- [Later] Spaceキーで一時キャッシュをmacOS Quick Lookへ渡し、プレビュー終了後に安全に削除
- [Later] 画像、PDF、動画などのサムネイルを低優先度で遅延生成し、メモリ・ディスク上限付きキャッシュへ保存
- [Later] 置換、連番、接頭辞・接尾辞、大文字・小文字変換を備えたプレビュー付き一括名称変更を実装
- [Later] 新規空ファイル、対応サーバーでのシンボリックリンク作成、フォルダ容量計算を追加
- [Later] ZIP／TARの圧縮アップロード、ダウンロード後展開、対応サーバーでのリモートアーカイブ操作を検討
- [Later] ローカルとリモートのテキストファイルを選択した比較ツールで開けるようにする
- [Later] クラウド事業者が提供するファイルバージョンの一覧、プレビュー、復元を共通UIへ統合
- [Later] S3署名付きURL、Google Driveなどの共有リンクを、有効期限と公開範囲を明示して作成

## Phase 14 — Advanced SSH and Authentication [Later]

**完了条件:** 踏み台、SSH Agent、多要素認証、企業ネットワークを含む高度な接続環境で、安全境界を維持して接続できる。

- [Later] `~/.ssh/config`からHost、HostName、User、Port、IdentityFileを読み込み、エイリアスを接続候補として利用
- [Later] macOSのSSH Agentを使う認証を追加し、秘密鍵素材をアプリへ取り込まない
- [Later] ProxyJumpによる踏み台接続を実装し、任意シェルを実行するProxyCommandは別途安全性を評価
- [Later] SOCKS5／HTTP CONNECTプロキシを接続先単位で設定可能にする
- [Later] SFTPのkeyboard-interactive、OTP／2FA認証を対話ダイアログへ統合
- [Later] FIDO2、YubiKey、PKCS#11、Secure Enclave鍵の対応可能性を調査
- [Later] FTPS／WebDAVのクライアント証明書認証と、証明書詳細表示を追加
- [Later] Implicit FTPSを独立した接続方式として追加
- [Later] SMBのKerberos、DFS、Bonjour検出を追加
- [Later] 接続テスト結果と秘密情報を除外したプロトコルトランスクリプトを表示・保存可能にする

## Phase 15 — Cloud Services [Later]

**完了条件:** 利用頻度の高いクラウドストレージを、各事業者のOAuth・権限・バージョン・共有モデルに合わせて安全に利用できる。

- [Later] Microsoft OneDrive、OneDrive for Business、SharePointを最優先の追加クラウドとして実装
- [Later] DropboxをOAuth、共有フォルダ、バージョン、共有リンク対応で実装
- [Later] Google Cloud StorageネイティブAPIを、Cloud FTPやGoogle Driveとは別の接続方式として実装
- [Later] Backblaze B2をapplication keyのbucket制限とversion対応で実装
- [Later] BoxをOAuth、企業フォルダ、version対応で実装
- [Later] Azure Blob Storage／Azure Filesをaccount key、SAS、有効期限表示に対応して実装
- [Later] OpenStack Swiftをtenant、region、Keystone認証に対応して実装
- [Later] Cloudflare R2などS3互換サービスは専用アダプタを増やさず、接続プロファイルで既定endpointと説明を提供
- [Later] 各OAuth接続は利用者所有Client IDを基本方針とし、tokenをKeychain以外へ保存しない

## Phase 16 — Automation and Security [Later]

**完了条件:** GUIと同じ安全なRustコアを、定期処理・スクリプト・暗号化ストレージから再利用できる。

- [Later] GUIと同じ接続、転送、同期、検証ロジックを呼び出すHeadless CLIを追加
- [Later] macOS launchdまたはアプリ内スケジューラから、保存済み同期計画を定期実行
- [Later] ローカルフォルダの変更を監視し、debounce、競合確認、停止を備えた自動アップロードを実装
- [Later] ファイルをドロップするだけで指定ブックマークへアップロードするDroplet／監視フォルダを追加
- [Later] macOSショートカット／URL Schemeから、ブックマーク接続、アップロード、同期プレビューを起動可能にする
- [Later] 自動処理の終了コード、構造化ログ、macOS通知を追加し、秘密情報を標準出力へ含めない
- [Later] Cryptomator互換vaultの作成、ロック解除、名前・内容暗号化、Keychainパスフレーズ保存を実装
- [Later] ブックマークと非機密設定を利用者所有ストレージへエンドツーエンド暗号化して同期する方式を設計
- [Later] S3 Object Lock、versioning、Google Drive trashなど復元可能な削除機構を操作前に表示

## 長期保留項目

- [Later] 削除を含む完全自動の双方向同期は、versioning、recycle bin、競合履歴、復元手段が揃うまで実装しない
- [Later] Finderへリモートをディスクとしてマウントする機能は、File Provider、オフラインキャッシュ、変更競合、容量管理を含む独立プロジェクト規模として再評価
- [Later] アプリ内SSHターミナルはファイル転送製品の責務を広げるため、転送・同期・接続診断が成熟するまで実装しない
- [Later] 新しいプロトコルの追加だけを優先せず、既存プロトコルの再開、検証、エラー回復、統合テストを先に完成させる
